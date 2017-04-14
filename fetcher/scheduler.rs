use std::thread;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::mpsc::RecvTimeoutError::{Timeout, Disconnected};
use std::collections::BinaryHeap;
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::cmp::Ordering;

use futures::Stream;
use futures::sync::mpsc::{unbounded, UnboundedSender};

pub struct Scheduler<T>(Sender<(u64, T)>);

impl<T: Send + 'static> Scheduler<T> {
    pub fn new() -> (Scheduler<T>, impl Stream<Item=T, Error=()>) {
        let (thrd_tx, thrd_rx) = channel();
        let (fut_tx, fut_rx) = unbounded();

        thread::spawn(move || worker(thrd_rx, fut_tx));

        (Scheduler(thrd_tx), fut_rx)
    }

    pub fn schedule(&self, delay: u64, payload: T) {
        let _ = self.0.send((delay, payload));
    }
}

struct Unit<T>(i64, T);

impl<T> Unit<T> {
    fn new(timestamp: u64, payload: T) -> Unit<T> {
        Unit(-(timestamp as i64), payload)
    }

    fn since(&self, now: u64) -> Duration {
        let timestamp = -self.0 as u64;
        let since = timestamp.saturating_sub(now);
        let (s, ns) = (since / 1000, (since % 1000) * 1000000);

        Duration::new(s, ns as u32)
    }
}

impl<T> PartialEq for Unit<T> {
    fn eq(&self, other: &Unit<T>) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Unit<T> {}

impl<T> PartialOrd for Unit<T> {
    fn partial_cmp(&self, other: &Unit<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Unit<T> {
    fn cmp(&self, other: &Unit<T>) -> Ordering {
        self.0.cmp(&other.0)
    }
}

fn worker<T>(rx: Receiver<(u64, T)>, tx: UnboundedSender<T>) {
    let mut heap = BinaryHeap::new();
    let mut pending = None;

    loop {
        let now = now();

        let incoming = rx.try_iter()
            .chain(pending.take())
            .map(|(delay, payload)| Unit::new(now + delay, payload));

        heap.extend(incoming);

        let leeway = heap.peek().map(|unit| unit.since(now));

        if let Some(leeway) = leeway {
            match rx.recv_timeout(leeway) {
                Ok(pair) => {
                    pending = Some(pair);
                    continue;
                },
                Err(Disconnected) => {
                    thread::sleep(leeway);
                },
                Err(Timeout) => {}
            };
        } else if let Ok(pair) = rx.recv() {
            pending = Some(pair);
            continue;
        } else {
            break;
        }

        let payload = heap.pop().unwrap().1;

        if tx.send(payload).is_err() {
            break;
        }
    }
}

fn now() -> u64 {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    ts.as_secs() * 1000 + ts.subsec_nanos() as u64 / 1000000
}

#[test]
fn it_schedules_initial() {
    use std::time::Instant;

    use futures::Future;

    let (scheduler, stream) = Scheduler::new();

    let start = Instant::now();

    scheduler.schedule(0, 0);
    scheduler.schedule(10, 2);
    scheduler.schedule(15, 3);
    scheduler.schedule(5, 1);

    let (ids, times): (Vec<_>, Vec<_>) = stream
        .take(4)
        .map(|id| (id, start.elapsed().subsec_nanos() / 1000000))
        .collect().wait().ok().unwrap()
        .into_iter()
        .unzip();

    assert_eq!(ids, [0, 1, 2, 3]);
    assert!(4 <= times[1] && times[1] < 8);
    assert!(9 <= times[2] && times[2] < 13);
    assert!(14 <= times[3] && times[3] < 18);
}

#[test]
fn it_shedules_incoming() {
    use std::time::Instant;

    use futures::Future;

    let (scheduler, stream) = Scheduler::new();

    let start = Instant::now();

    thread::sleep(Duration::from_millis(10));
    scheduler.schedule(0, 0);

    thread::sleep(Duration::from_millis(5));
    scheduler.schedule(15, 3);

    thread::sleep(Duration::from_millis(5));
    scheduler.schedule(5, 2);
    scheduler.schedule(0, 1);

    let (ids, times): (Vec<_>, Vec<_>) = stream
        .take(4)
        .map(|id| (id, start.elapsed().subsec_nanos() / 1000000))
        .collect().wait().ok().unwrap()
        .into_iter()
        .unzip();

    assert_eq!(ids, [0, 1, 2, 3]);
    assert!(19 <= times[0] && times[0] < 23);
    assert!(19 <= times[1] && times[1] < 23);
    assert!(24 <= times[2] && times[2] < 28);
    assert!(29 <= times[3] && times[3] < 33);
}

#[test]
fn it_delivers_after_disconnect() {
    use futures::Future;

    let (scheduler, stream) = Scheduler::new();

    scheduler.schedule(20, 0);
    scheduler.schedule(21, 1);
    scheduler.schedule(22, 2);
    scheduler.schedule(23, 3);

    drop(scheduler);

    let ids = stream.collect().wait().ok().unwrap();

    assert_eq!(ids, [0, 1, 2, 3]);
}

#[test]
fn it_stops_worker_after_stream_closing() {
    let (scheduler, _) = Scheduler::new();

    scheduler.schedule(0, 0);

    thread::sleep(Duration::from_millis(2));

    assert!(scheduler.0.send((0, 0)).is_err());
}
