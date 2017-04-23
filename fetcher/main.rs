#![feature(conservative_impl_trait)]

extern crate common;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate tokio_request;
extern crate rss;
extern crate mailparse;
extern crate time;
extern crate uuid;
extern crate diesel;
extern crate readability;

use std::cmp;
use std::ascii::AsciiExt;
use std::io::Error as IoError;

use time::Timespec;
use tokio_core::reactor::{Core, Handle};
use futures::future;
use futures::{Future, Stream};
use rss::Channel;
use uuid::{Uuid, NAMESPACE_X500};
use diesel::prelude::*;
use diesel::pg::PgConnection;

use common::logger;
use common::models::{Feed, NewEntry};
use common::schema;
use common::types::{Url, Key};
use scheduler::Scheduler;
use readability::Readability;

mod scheduler;
mod download;

const MIN_INTERVAL: u32 = 3600;
const MAX_INTERVAL: u32 = 24 * 3600;
const PROMPTNESS: f32 = 0.5;

fn estimate_interval(prev: u32, total: u32, new: u32) -> u32 {
    if total == 0 {
        return cmp::min(prev + MIN_INTERVAL, MAX_INTERVAL);
    }

    let staleness = (total - new) as f32 / total as f32;
    let delta = staleness - PROMPTNESS;
    let estimated = prev as f32 * (1. + delta / PROMPTNESS);
    let trust = (total as f32 / 30.).min(1.);

    let next = prev as f32 * (1. - trust) + estimated * trust;

    cmp::max(MIN_INTERVAL, cmp::min(next as u32, MAX_INTERVAL))
}

fn purify_text(string: String) -> Option<String> {
    if !string.is_empty() && string.trim().len() == string.len() {
        return Some(string);
    }

    let string = string.trim();

    if string.is_empty() {
        None
    } else {
        Some(string.to_owned())
    }
}

fn unify_language(mut language: String) -> Option<String> {
    if !language.is_ascii() {
        warn!("Cannot convert \"{}\" to a language code: not ascii", language);
        return None;
    }

    if language.len() == 2 {
        language.make_ascii_lowercase();
        Some(language)
    } else if language.len() > 2 {
        Some(language.trim()[..2].to_lowercase())
    } else {
        warn!("Cannot convert \"{}\" to a language code", language);
        None
    }
}

fn parse_rfc822_date(date: &str) -> Option<Timespec> {
    match mailparse::dateparse(date) {
        Ok(date) => Some(Timespec::new(date, 0)),
        Err(err) => {
            warn!("Cannot parse \"{}\" as date: {}", date, err);
            None
        }
    }
}

fn parse_url(url: &str) -> Option<Url> {
    match Url::parse(url.trim()) {
        Ok(url) => Some(url),
        Err(_) => {
            warn!("Cannot parse \"{}\" as url", url);
            None
        }
    }
}

fn fetch_entries(handle: &Handle, feed: Feed)
    -> impl Future<Item=(Feed, Vec<NewEntry>), Error=IoError>
{
    info!("Fetching {} feed...", feed.key);

    download::channel(handle, &feed.url).map(|channel| disassemble_channel(feed, channel))
}

fn disassemble_channel(mut feed: Feed, channel: Channel) -> (Feed, Vec<NewEntry>) {
    // TODO(loyd): use `channel.ttl` as some assumption about `feed.interval`.
    feed.title = purify_text(channel.title);
    feed.description = purify_text(channel.description);
    feed.language = channel.language.and_then(unify_language);
    feed.logo = channel.image.and_then(|image| Url::parse(&image.url).ok());
    feed.copyright = channel.copyright.and_then(purify_text);

    let augmented = feed.augmented.unwrap_or(Timespec::new(0, 0));

    let (mut total_count, mut new_count) = (0, 0);

    let entries = channel.items.into_iter().filter_map(|item| {
        let published = item.pub_date.and_then(|date| parse_rfc822_date(&date));

        if let Some(published) = published {
            total_count += 1;

            if published <= augmented {
                return None;
            }

            new_count += 1;
        }

        let url = item.link.and_then(|url| parse_url(&url));

        let key = if let Some(ref url) = url {
            Key::from(url.clone())
        } else if let Some(ref title) = item.title {
            Key::from(Uuid::new_v5(&NAMESPACE_X500, title))
        } else {
            return None;
        };

        Some(NewEntry {
            key,
            url,
            published,
            feed_id: feed.id,
            title: item.title.and_then(purify_text),
            author: item.author.and_then(purify_text),
            description: item.description.and_then(purify_text),
            content: item.content.and_then(purify_text)
        })
    }).collect();

    let prev_interval = feed.interval.unwrap_or(0) as u32;
    feed.interval = Some(estimate_interval(prev_interval, total_count, new_count) as i32);

    (feed, entries)
}

fn fetch_documents(handle: &Handle, feed: Feed, entries: Vec<NewEntry>)
    -> impl Future<Item=(Feed, Vec<NewEntry>), Error=IoError> + 'static
{
    type BoxFuture = Box<Future<Item=NewEntry, Error=IoError>>;

    // TODO(loyd): ignore fails.
    let fetchers = entries.into_iter().map(|mut entry| {
        debug!("  Fetching {} entry...", entry.key);

        if entry.url.is_none() {
            return Box::new(future::ok(entry)) as BoxFuture;
        }

        let fetcher = download::document(handle, entry.url.as_ref().unwrap());

        let future = fetcher.map(|document| {
            let content = Readability::new().parse(&document).to_string();

            // TODO(loyd): leave original `content` in some situations.
            entry.content = Some(content);
            entry
        });

        Box::new(future) as BoxFuture
    }).collect::<Vec<_>>();

    future::join_all(fetchers).map(|entries| (feed, entries))
}

fn save_entries(connection: &PgConnection, feed: &Feed, entries: &[NewEntry]) -> QueryResult<bool> {
    connection.transaction(|| {
        let active = diesel::update(feed)
            .set(feed)
            .execute(connection)? > 0;

        diesel::insert(entries)
            .into(schema::entry::table)
            .execute(connection)?;

        Ok(active)
    })
}

fn main() {
    logger::init().unwrap();

    let connection = schema::establish_connection().unwrap();

    let (scheduler, feed_stream) = Scheduler::new();

    info!("Loading feeds...");

    let initial_feeds = schema::feed::table.load::<Feed>(&connection).unwrap();

    info!("Scheduling initial {} feeds...", initial_feeds.len());

    for feed in initial_feeds {
        let timeout = feed.interval.unwrap_or(0);

        debug!("  Scheduled {} after {}s", feed.key, timeout);

        scheduler.schedule((timeout * 1000) as u64, feed);
    }

    info!("Running the reactor...");

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let process = feed_stream
        .then(|feed| fetch_entries(&handle, feed.unwrap()))
        .and_then(|(feed, entries)| fetch_documents(&handle, feed, entries))
        .for_each(|(mut feed, entries)| {
            info!("Visited {} and collected {} new entries", feed.key, entries.len());

            if let Some(augmented) = entries.iter().filter_map(|entry| entry.published).max() {
                feed.augmented = Some(augmented);
            }

            if save_entries(&connection, &feed, &entries).unwrap() {
                debug!("  Scheduled after {}s", feed.interval.unwrap());

                scheduler.schedule((feed.interval.unwrap() * 1000) as u64, feed);
            }

            Ok(())
        });

    lp.run(process).unwrap();
}

#[test]
fn it_estimates_interval() {
    assert_eq!(estimate_interval(MIN_INTERVAL, 30, 0), (MIN_INTERVAL as f32 / PROMPTNESS) as u32);

    // Keypoints.
    let some_prev = MIN_INTERVAL + 2048;
    assert_eq!(estimate_interval(some_prev, 10000, ((1. - PROMPTNESS) * 10000.) as u32), some_prev);
    assert_eq!(estimate_interval(MIN_INTERVAL, 30, 30), MIN_INTERVAL);
    assert_eq!(estimate_interval(MIN_INTERVAL, 1, 1), MIN_INTERVAL);
    assert_eq!(estimate_interval(MAX_INTERVAL, 30, 0), MAX_INTERVAL);
    assert_eq!(estimate_interval(MAX_INTERVAL, 1, 0), MAX_INTERVAL);
    assert_eq!(estimate_interval(MIN_INTERVAL + 42, 0, 0), 2 * MIN_INTERVAL + 42);
    assert_eq!(estimate_interval(0, 30, 30), MIN_INTERVAL);
    assert_eq!(estimate_interval(0, 30, 0), MIN_INTERVAL);
    assert_eq!(estimate_interval(0, 1, 0), MIN_INTERVAL);
    assert_eq!(estimate_interval(0, 0, 0), MIN_INTERVAL);
}
