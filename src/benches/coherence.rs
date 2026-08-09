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
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

// Own the whole cache line (128 B on Apple Silicon, covers 64 B too) so
// nothing else shares it (false sharing would distort the measurement).
#[repr(align(128))]
struct Line(AtomicU64);

#[repr(align(128))]
struct Flag(AtomicBool);

/// Baseline: atomic RMW on a line nobody else touches — stays in own L1.
fn uncontended() -> Stat {
    const ITERS: u64 = 4_000_000;
    let a = Line(AtomicU64::new(0));
    bench(
        "atomic: fetch_add, uncontended",
        "single thread, line stays in own L1",
        7,
        ITERS,
        || {
            for _ in 0..ITERS {
                black_box(a.0.fetch_add(1, Ordering::Relaxed));
            }
        },
    )
}

/// One thread streams stores into the atomic as fast as it can; the
/// measured thread polls it with Acquire loads. Each fresh value means the
/// reader's cached copy was invalidated and the line must be re-fetched
/// from the writer's cache — the reader-side cost of the pattern discussed
/// as "one writer, one reader". The writer barely stalls (stores retire
/// into its store buffer); the reader pays the transfers.
fn one_writer_one_reader() -> Stat {
    const READS: u64 = 1_000_000;

    let value = Arc::new(Line(AtomicU64::new(0)));
    let stop = Arc::new(Flag(AtomicBool::new(false)));

    let writer = {
        let value = Arc::clone(&value);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut n = 1u64;
            while !stop.0.load(Ordering::Relaxed) {
                value.0.store(n, Ordering::Release);
                n = n.wrapping_add(1);
            }
        })
    };

    let stat = bench(
        "atomic: read while other core writes",
        "1 writer / 1 reader, reader-side poll cost",
        7,
        READS,
        || {
            let mut acc = 0u64;
            for _ in 0..READS {
                acc ^= value.0.load(Ordering::Acquire);
            }
            black_box(acc);
        },
    );

    stop.0.store(true, Ordering::Relaxed);
    writer.join().unwrap();

    stat
}

fn ping_pong() -> Stat {
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

    stat
}

pub fn run() -> Vec<Stat> {
    vec![uncontended(), one_writer_one_reader(), ping_pong()]
}
