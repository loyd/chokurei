#![feature(conservative_impl_trait)]

extern crate common;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate tokio_request;
extern crate rss;
extern crate url;
extern crate kuchiki;

use tokio_core::reactor::Core;
use url::Url;

use common::logger;

mod scheduler;
mod download;

fn main() {
    logger::init().unwrap();

    let mut lp = Core::new().unwrap();
    let url = Url::parse("http://habrahabr.ru/rss/hub/c/").unwrap();

    let request = download::channel(&lp, &url);

    let data = lp.run(request).unwrap();

    info!("Visit {}", data.link);

    let url = Url::parse(&data.link).unwrap();

    let request = download::document(&lp, &url);

    let data = lp.run(request).unwrap();

    info!("{}", data.to_string());
}
