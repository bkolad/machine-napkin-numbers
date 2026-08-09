//! Benchmark 5: [u8; 32] heap allocation + deallocation.
//!
//! Box::new + drop per iteration measures the system allocator's
//! small-allocation fast path (macOS: libmalloc nano allocator).
//! black_box on the pointer keeps LLVM from eliding the malloc/free pair.

use crate::harness::{bench, Stat};
use std::hint::black_box;

pub fn run() -> Vec<Stat> {
    const ITERS: u64 = 4_000_000;

    let boxed = bench(
        "alloc: Box<[u8;32]> new + drop",
        "malloc+free fast path, 32 B",
        7,
        ITERS,
        || {
            for _ in 0..ITERS {
                let b = Box::new(black_box([7u8; 32]));
                black_box(b.as_ptr());
                drop(b);
            }
        },
    );

    let vec = bench(
        "alloc: Vec::with_capacity(32) + drop",
        "same, via Vec<u8>",
        7,
        ITERS,
        || {
            for _ in 0..ITERS {
                let v: Vec<u8> = Vec::with_capacity(black_box(32));
                black_box(v.as_ptr());
                drop(v);
            }
        },
    );

    vec![boxed, vec]
}
