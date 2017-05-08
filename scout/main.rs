extern crate common;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate time;
extern crate serde_json;
extern crate kafka;
extern crate url;

use std::str;
use std::net::SocketAddr;
use std::io::{Result as IoResult, Error as IoError, ErrorKind as IoErrorKind};
use std::collections::HashSet;

use url::Url;
use time::Timespec;
use tokio_core::reactor::Core;
use tokio_core::net::{UdpSocket, UdpCodec};
use futures::Stream;
use kafka::consumer::{Consumer, FetchOffset};
use kafka::producer::{Producer, Record, Partitioner};

use common::logger;
use common::key::Key;
use common::messages::Feed;

const KAFKA_URL: &str = "127.0.0.1:9092";
const ADDING_URL: &str = "127.0.0.1:3042";

struct UrlCodec;

impl UdpCodec for UrlCodec {
    type In = Url;
    type Out = ();

    fn decode(&mut self, _: &SocketAddr, buffer: &[u8]) -> IoResult<Url> {
        let message = str::from_utf8(buffer).map_err(|_| IoErrorKind::InvalidData)?;

        let line = message.lines().next()
            .ok_or(IoErrorKind::InvalidData)?
            .trim();

        let url = Url::parse(line)
            .map_err(|cause| IoError::new(IoErrorKind::InvalidInput, cause))?;

        Ok(url)
    }

    fn encode(&mut self, _: (), _: &mut Vec<u8>) -> SocketAddr {
        unimplemented!();
    }
}

fn recv_known_keys() -> HashSet<Key> {
    let mut consumer = Consumer::from_hosts(vec![KAFKA_URL.to_owned()])
        .with_fallback_offset(FetchOffset::Earliest)
        .with_topic("feeds".to_owned())
        .create().unwrap();

    let message_sets = consumer.poll().unwrap();

    message_sets.iter()
        .flat_map(|ms| ms.messages())
        .filter_map(|message| String::from_utf8(message.key.to_vec()).ok())
        .map(Key::from)
        .collect()
}

fn send_feed<P: Partitioner>(producer: &mut Producer<P>, key: Key, url: Url) {
    let feed = Feed {
        url,
        source: Url::parse("http://unknown").unwrap(),
        interval: 0,
        augmented: Timespec::new(0, 0)
    };

    let key: String = key.into();
    let value = serde_json::to_vec(&feed).unwrap();

    producer.send(&Record::from_key_value("feeds", key, value)).unwrap();
}

fn main() {
    logger::init().unwrap();

    let mut producer = Producer::from_hosts(vec![KAFKA_URL.to_owned()]).create().unwrap();

    info!("Receiving known keys...");

    let mut known_keys = recv_known_keys();

    info!("Start adding new feeds from {}...", ADDING_URL);

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let addr = &ADDING_URL.parse().unwrap();

    let adding = UdpSocket::bind(addr, &handle).unwrap().framed(UrlCodec)
        .map(|url| {
            // TODO: get rid of cloning here and in the raider.
            let key = Key::from(url.clone());

            if known_keys.contains(&key) {
                warn!("{} ({}) is already in topic", key, url);
                return ();
            }

            info!("Added {} ({})", key, url);

            known_keys.insert(key.clone());
            send_feed(&mut producer, key, url);

            ()
        })
        .or_else(|error| {
            error!("Invalid url: {}", error);
            Ok::<(), ()>(())
        })
        .for_each(|_| Ok(()));

    lp.run(adding).unwrap();
}
