#![feature(conservative_impl_trait)]

extern crate common;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate tokio_request;
extern crate rss;
extern crate time;
extern crate mailparse;
extern crate readability;
extern crate serde_json;
extern crate kafka;
extern crate url;

use std::cmp;
use std::thread;

use time::Timespec;
use tokio_core::reactor::{Core, Handle};
use futures::future;
use futures::{Future, Stream};
use rss::Channel;
use url::Url;
use readability::Readability;
use kafka::consumer::{Consumer, FetchOffset};
use kafka::producer::{Producer, Record, Partitioner};

use common::logger;
use common::key::Key;
use common::messages::{Feed, Entry};
use scheduler::Scheduler;

mod scheduler;
mod download;

const KAFKA_URL: &str = "localhost:9092";
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

fn parse_rfc822_date(date: &str) -> Option<Timespec> {
    match mailparse::dateparse(date) {
        Ok(date) => Some(Timespec::new(date, 0)),
        Err(error) => {
            warn!("Cannot parse \"{}\" as date: {}", date, error);
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

fn fetch_entries(handle: &Handle, mut feed: Feed)
    -> impl Future<Item=(Feed, Vec<Entry>), Error=()>
{
    info!("Fetching {} feed...", feed.url);

    download::channel(handle, &feed.url).then(|channel| {
        Ok(match channel {
            Ok(channel) => disassemble_channel(feed, channel),
            Err(error) => {
                warn!("Fetching {} is failed: {}", feed.url, error);

                feed.interval = estimate_interval(feed.interval, 0, 0);

                (feed, Vec::new())
            }
        })
    })
}

fn disassemble_channel(mut feed: Feed, channel: Channel) -> (Feed, Vec<Entry>) {
    let (mut total_count, mut new_count) = (0, 0);

    feed.source = parse_url(&channel.link).unwrap_or(feed.source);

    let entries = channel.items.into_iter().filter_map(|item| {
        let url = match item.link.and_then(|url| parse_url(&url)) {
            Some(url) => url,
            None => {
                warn!("Got item without link from {}", feed.url);
                return None;
            }
        };

        let published = match item.pub_date.and_then(|date| parse_rfc822_date(&date)) {
            Some(published) => published,
            None => {
                warn!("Got item {} without published date from {}", url, feed.url);
                return None;
            }
        };

        let title = match item.title.and_then(purify_text) {
            Some(title) => title,
            None => {
                warn!("Got item {} without title from {}", url, feed.url);
                return None;
            }
        };

        total_count += 1;

        if published <= feed.augmented {
            return None;
        }

        new_count += 1;

        let description = item.description.and_then(purify_text);
        let content = item.content.and_then(purify_text);

        Some(Entry {
            url,
            title,
            published,
            source: feed.source.clone(),
            author: item.author.and_then(purify_text),
            content: content.or(description).unwrap_or_else(String::new)
        })
    }).collect();

    // TODO: use `channel.ttl` as some assumption about `feed.interval`.
    feed.interval = estimate_interval(feed.interval, total_count, new_count);

    (feed, entries)
}

fn fetch_documents(handle: &Handle, feed: Feed, entries: Vec<Entry>)
    -> impl Future<Item=(Feed, Vec<Entry>), Error=()> + 'static
{
    let fetchers = entries.into_iter().map(|mut entry| {
        debug!("  Fetching {} entry...", entry.url);

        let download = download::document(handle, &entry.url);

        download.then(|result| {
            let document = match result {
                Ok(document) => document,
                Err(error) => {
                    warn!("Fetching {} is failed: {}", entry.url, error);
                    return Ok(None);
                }
            };

            // TODO: should we use a thread pool here?
            let content = Readability::new().parse(&document).text_contents();

            // TODO: leave original `content` in some situations.
            entry.content = content;
            Ok(Some(entry))
        })
    }).collect::<Vec<_>>();

    future::join_all(fetchers).map(|entries| {
        let entries = entries.into_iter().filter_map(|entry| entry).collect();
        (feed, entries)
    })
}

fn scheduling(scheduler: Scheduler<Feed>) {
    let mut consumer = Consumer::from_hosts(vec![KAFKA_URL.to_owned()])
        .with_fallback_offset(FetchOffset::Earliest)
        .with_topic("feeds".to_owned())
        .create().unwrap();

    info!("Start scheduling...");

    loop {
        for message_set in consumer.poll().unwrap().iter() {
            for message in message_set.messages() {
                let feed = match serde_json::from_slice::<Feed>(message.value) {
                    Ok(feed) => feed,
                    Err(error) => {
                        error!("Invalid message on \"feeds\" topic: {}", error);
                        continue;
                    }
                };

                info!("Scheduling {} after {}s...", feed.url, feed.interval);
                scheduler.schedule(feed.interval as u64 * 1000, feed);
            }

            consumer.consume_messageset(message_set).unwrap();
        }

        consumer.commit_consumed().unwrap();
    }
}

fn send_feed<P>(producer: &mut Producer<P>, feed: Feed)
    where P: Partitioner
{
    let key: String = Key::from(feed.url.clone()).into();
    let value = serde_json::to_vec(&feed).unwrap();

    producer.send(&Record::from_key_value("feeds", key, value)).unwrap();
}

fn send_entries<P>(producer: &mut Producer<P>, entries: Vec<Entry>)
    where P: Partitioner
{
    let records = entries.into_iter()
        .map(|entry| Record::from_value("entries", serde_json::to_vec(&entry).unwrap()))
        .collect::<Vec<_>>();

    producer.send_all(&records).unwrap();
}

fn fetching<S>(stream: S)
    where S: Stream<Item=Feed, Error=()>
{
    let mut producer = Producer::from_hosts(vec![KAFKA_URL.to_owned()]).create().unwrap();

    info!("Start fetching...");

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let process = stream
        // TODO: ideally, in the case of redirect, we should change the url.
        // TODO: should we fetch feeds concurrently?
        .and_then(|feed| fetch_entries(&handle, feed))
        .and_then(|(feed, entries)| fetch_documents(&handle, feed, entries))
        .for_each(|(mut feed, entries)| {
            info!("Visited {} and collected {} new entries", feed.url, entries.len());

            if let Some(augmented) = entries.iter().map(|entry| entry.published).max() {
                feed.augmented = augmented;
            }

            send_feed(&mut producer, feed);
            send_entries(&mut producer, entries);

            Ok(())
        });

    lp.run(process).unwrap();
}

fn main() {
    logger::init().unwrap();

    let (scheduler, stream) = Scheduler::new();

    thread::spawn(move || scheduling(scheduler));
    fetching(stream);
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
