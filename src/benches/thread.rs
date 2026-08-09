//! Benchmark 6: thread context switch.
//!
//! Two threads ping-pong a message over a pair of mpsc channels. Each
//! round trip forces both threads to block and be rescheduled, i.e. two
//! context switches. macOS exposes no thread-affinity API, so this is the
//! scheduler-mediated cost on whatever cores the OS picks (P vs E cores
//! can move the number).

use crate::harness::{bench, Stat};
use std::sync::mpsc;
use std::thread;

pub fn run() -> Vec<Stat> {
    const ROUNDS: u64 = 10_000;

    let (tx_a, rx_a) = mpsc::channel::<()>();
    let (tx_b, rx_b) = mpsc::channel::<()>();

    let worker = thread::spawn(move || {
        while rx_a.recv().is_ok() {
            if tx_b.send(()).is_err() {
                break;
            }
        }
    });

    let stat = bench(
        "thread: context switch (ping-pong)",
        "channel round trip / 2, incl. wake+schedule",
        7,
        ROUNDS * 2, // two switches per round trip
        || {
            for _ in 0..ROUNDS {
                tx_a.send(()).unwrap();
                rx_b.recv().unwrap();
            }
        },
    );

    drop(tx_a); // worker's recv() errors and it exits
    worker.join().unwrap();

    vec![stat]
}
