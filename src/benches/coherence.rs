//! Benchmark 7: passing a cache line between CPU cores.
//!
//! Two threads spin on the same atomic counter, taking turns incrementing
//! it (main writes even -> odd, worker odd -> even). Every store invalidates
//! the line in the other core's cache, so each turn forces one ownership
//! transfer through the coherence protocol. Unlike the thread-switch bench
//! there are no syscalls and nobody blocks — this isolates pure
//! cache-to-cache latency. Round trip / 2 = one transfer.
//!
//! No thread pinning on macOS: which cores (same cluster, cross-cluster,
//! or cross-CCD on chiplet CPUs) the two threads land on varies by run and
//! moves this number.

use crate::harness::{bench, Stat};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

// Own the whole cache line (128 B on Apple Silicon, covers 64 B too) so
// nothing else shares it (false sharing would distort the measurement).
#[repr(align(128))]
struct Line(AtomicU64);

pub fn run() -> Vec<Stat> {
    const ROUNDS: u64 = 200_000;

    let line = Arc::new(Line(AtomicU64::new(0)));
    let stop = Arc::new(AtomicBool::new(false));

    let worker = {
        let line = Arc::clone(&line);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut turn = 1u64;
            loop {
                while line.0.load(Ordering::Acquire) != turn {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::hint::spin_loop();
                }
                line.0.store(turn + 1, Ordering::Release);
                turn += 2;
            }
        })
    };

    let mut turn = 0u64;
    let stat = bench(
        "coherence: cache line core-to-core",
        "spin ping-pong on one line, round trip / 2",
        7,
        ROUNDS * 2, // two ownership transfers per round trip
        || {
            for _ in 0..ROUNDS {
                while line.0.load(Ordering::Acquire) != turn {
                    std::hint::spin_loop();
                }
                line.0.store(turn + 1, Ordering::Release);
                turn += 2;
            }
        },
    );

    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();

    vec![stat]
}
