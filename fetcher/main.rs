extern crate common;
#[macro_use]
extern crate log;

use std::thread;
use std::time::Duration;

use common::logger;

fn main() {
    logger::init().unwrap();

    loop {
        info!("Hello from fetcher!");
        thread::sleep(Duration::new(60, 0));
    }
}
