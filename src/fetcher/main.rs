use std::thread;
use std::time::Duration;

fn main() {
    loop {
        println!("Hello from fetcher!");
        thread::sleep(Duration::new(60, 0));
    }
}
