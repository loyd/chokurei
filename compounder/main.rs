extern crate common;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate futures;
extern crate tokio_core;
extern crate time;
extern crate serde_json;
extern crate kafka;
extern crate url;
extern crate rust_stemmers;

use std::sync::Mutex;
use std::collections::HashMap;
use std::thread;

use kafka::consumer::{Consumer, FetchOffset};
use kafka::producer::{Producer, Record};
use url::Url;

use common::logger;
use common::messages::Entry;
use document::Document;

mod document;

const KAFKA_URL: &str = "127.0.0.1:9092";

struct Cluster {
    documents: Vec<Document>,
    fake: bool,
}

impl Cluster {
    pub fn new() -> Cluster {
        Cluster {
            documents: Vec::new(),
            fake: false,
        }
    }

    pub fn distance_to_document(&self, target: &Document) -> f32 {
        let count = self.documents.len();

        if count == 0 {
            return 0.;
        }

        self.documents.iter()
            .map(|member| target.distance(member))
            .sum::<f32>() / count as f32
    }

    pub fn add_document(&mut self, document: Document) {
        if self.fake {
            report_about_fake(&document);
        }

        self.documents.push(document);
    }

    pub fn mark_as_fake(&mut self) {
        if self.fake {
            return;
        }

        for document in &self.documents {
            report_about_fake(&document);
        }
    }
}

lazy_static! {
    static ref CLUSTERS: Mutex<Vec<Cluster>> = Mutex::new(Vec::new());

    static ref URL_TO_CLUSTER: Mutex<HashMap<Url, usize>> = Mutex::new(HashMap::new());

    static ref PRODUCER: Mutex<Producer> = Mutex::new(Producer::from_hosts(vec![KAFKA_URL.to_owned()]).create().unwrap());
}

fn add_document(document: Document) {
    // TODO: lookup clusters using position if document on an unit sphere.
    //              O(1) instead of O(n) here!

    let mut clusters = CLUSTERS.lock().unwrap();

    let candidate = find_candidate(&*clusters, &document);

    let url = document.entry().url.clone();

    let idx = if let Some(idx) = candidate {
        let cluster = &mut clusters[idx];
        cluster.add_document(document);

        idx
    } else {
        let mut cluster = Cluster::new();
        cluster.add_document(document);

        clusters.push(cluster);

        clusters.len() - 1
    };

    URL_TO_CLUSTER.lock().unwrap().insert(url, idx);
}

fn find_candidate(clusters: &[Cluster], document: &Document) -> Option<usize> {
    clusters.iter()
        .enumerate()
        .map(|(idx, cluster)| (idx, cluster.distance_to_document(&document)))
        .filter(|&(_, distance)| distance > 0.7)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(idx, _)| idx)
}

fn report_about_fake(document: &Document) {
    let record = Record::from_value("fakes", serde_json::to_vec(document.entry()).unwrap());

    PRODUCER.lock().unwrap().send(&record).unwrap();
}

fn fake_adding() {
    let mut consumer = Consumer::from_hosts(vec![KAFKA_URL.to_owned()])
        .with_fallback_offset(FetchOffset::Earliest)
        .with_topic("expert".to_owned())
        .create().unwrap();

    info!("Waiting for new fakes from expert...");

    loop {
        // TODO: rewrite in functional style.
        for message_set in consumer.poll().unwrap().iter() {
            for message in message_set.messages() {
                let url = match serde_json::from_slice::<String>(message.value) {
                    Ok(url) => url,
                    Err(error) => {
                        error!("Invalid message on \"expert\" topic: {}", error);
                        continue;
                    }
                };

                if let Ok(url) = Url::parse(&url) {
                    add_fake(url);
                } else {
                    error!("Unparsable url on \"expert\" topic: {}", url);
                }
            }

            consumer.consume_messageset(message_set).unwrap();
        }

        consumer.commit_consumed().unwrap();
    }
}

fn add_fake(url: Url) {
    let map = URL_TO_CLUSTER.lock().unwrap();

    if let Some(idx) = map.get(&url) {
        if let Some(cluster) = CLUSTERS.lock().unwrap().get_mut(*idx) {
            cluster.mark_as_fake();
        }
    }
}

fn document_adding() {
    let mut consumer = Consumer::from_hosts(vec![KAFKA_URL.to_owned()])
        .with_fallback_offset(FetchOffset::Earliest)
        .with_topic("entries".to_owned())
        .create().unwrap();

    info!("Waiting for new entries...");

    loop {
        // TODO: rewrite in functional style.
        for message_set in consumer.poll().unwrap().iter() {
            for message in message_set.messages() {
                let entry = match serde_json::from_slice::<Entry>(message.value) {
                    Ok(entry) => entry,
                    Err(error) => {
                        error!("Invalid message on \"entries\" topic: {}", error);
                        continue;
                    }
                };

                // TODO: parallize it!
                if let Some(document) = Document::from_entry(entry) {
                    add_document(document);
                }
            }

            consumer.consume_messageset(message_set).unwrap();
        }

        consumer.commit_consumed().unwrap();
    }
}

fn main() {
    logger::init().unwrap();

    thread::spawn(fake_adding);
    document_adding();
}
