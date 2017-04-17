#![feature(conservative_impl_trait)]

extern crate common;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate tokio_request;
extern crate rss;
extern crate kuchiki;
extern crate mailparse;
extern crate time;

use std::ascii::AsciiExt;
use std::io::Error as IoError;

use time::Timespec;
use tokio_core::reactor::{Core, Handle};
use futures::future;
use futures::{Future, Stream};
use rss::Channel;
use kuchiki::NodeRef;

use common::logger;
use common::schema;
use common::models::{Feed, NewEntry};
use common::types::{Url, Key};
use scheduler::Scheduler;

mod scheduler;
mod download;

fn retrieve_feeds() -> Vec<Feed> {
    // TODO(loyd): fetch feeds from the database.

    let url1 = Url::parse("http://habrahabr.ru/rss/hub/c/").unwrap();
    let url2 = Url::parse("http://habrahabr.ru/rss/hub/rust/").unwrap();

    vec![Feed {
        id: 0,
        key: Key::from(url1.clone()),
        url: url1,
        title: None,
        description: None,
        language: None,
        logo: None,
        copyright: None,
        interval: None,
        augmented: None
    }, Feed {
        id: 0,
        key: Key::from(url2.clone()),
        url: url2,
        title: None,
        description: None,
        language: None,
        logo: None,
        copyright: None,
        interval: Some(60),
        augmented: None
    }]
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
        return None;
    }

    if language.len() == 2 {
        language.make_ascii_lowercase();
        Some(language)
    } else if language.len() > 2 {
        Some(language.trim()[..2].to_lowercase())
    } else {
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

    let entries = channel.items.into_iter().filter_map(|item| {
        let published = item.pub_date.and_then(|date| parse_rfc822_date(&date));

        if let Some(published) = published {
            if published <= augmented {
                return None;
            }
        }

        let url = item.link.and_then(|url| parse_url(&url));

        // FIXME(loyd): get rid of `unwrap`.
        let key = Key::from(url.clone().unwrap());

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

    (feed, entries)
}

fn fetch_documents(handle: &Handle, feed: Feed, entries: Vec<NewEntry>)
    -> impl Future<Item=(Feed, Vec<NewEntry>), Error=IoError> + 'static
{
    type BoxFuture = Box<Future<Item=NewEntry, Error=IoError>>;

    // TODO(loyd): ignore fails.
    let fetchers = entries.into_iter().map(|mut entry| {
        if entry.url.is_none() {
            return Box::new(future::ok(entry)) as BoxFuture;
        }

        let fetcher = download::document(handle, entry.url.as_ref().unwrap());

        let future = fetcher.map(|document| {
            // TODO(loyd): use `readability.rs`, Luke!
            // TODO(loyd): leave original `content` in some situations.
            entry.content = Some(document.to_string());
            entry
        });

        Box::new(future) as BoxFuture
    }).collect::<Vec<_>>();

    future::join_all(fetchers).map(|entries| (feed, entries))
}

fn main() {
    logger::init().unwrap();

    let (scheduler, feeds) = Scheduler::new();

    for mut feed in retrieve_feeds() {
        feed.interval = feed.interval.or(Some(30));

        scheduler.schedule(feed.interval.unwrap() as u64 * 1000, feed);
    }

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let process = feeds
        .then(|feed| fetch_entries(&handle, feed.unwrap()))
        .and_then(|(feed, entries)| fetch_documents(&handle, feed, entries))
        .for_each(|(mut feed, entries)| {
            info!("Visited {} and collected {} new entries", feed.url, entries.len());

            if let Some(augmented) = entries.iter().filter_map(|entry| entry.published).max() {
                feed.augmented = Some(augmented);
            }

            // TODO(loyd): correct the interval.
            // TODO(loyd): save the feed and entries to the database.

            scheduler.schedule((feed.interval.unwrap() * 1000) as u64, feed);

            Ok(())
        });

    lp.run(process).unwrap();
}
