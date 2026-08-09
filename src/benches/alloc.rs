//! Benchmark 5: [u8; 32] allocation and deallocation.
//!
//! Stack: a [u8; 32] local forced to have an address via black_box. True
//! stack "allocation" is a stack-pointer bump paid once per frame (~0);
//! what this measures is materializing 32 bytes into the frame.
//!
//! Heap, split: allocation and deallocation timed as separate passes —
//! Box::into_raw stashes ITERS pointers (alloc pass), then Box::from_raw
//! frees them (dealloc pass). Batched behavior differs slightly from
//! paired alloc/free: the alloc pass grows the heap, the free pass fills
//! free lists.
//!
//! Heap, paired: Box::new + drop per iteration — the allocator's
//! steady-state fast path, closest to real code.

use crate::harness::{bench, Stat};
use std::hint::black_box;
use std::time::Instant;

const ITERS: u64 = 4_000_000;
const SPLIT_ITERS: usize = 1_000_000;

fn split_alloc_dealloc(samples: usize) -> (Stat, Stat) {
    let mut ptrs: Vec<*mut [u8; 32]> = Vec::with_capacity(SPLIT_ITERS);
    let mut alloc_ns = Vec::new();
    let mut free_ns = Vec::new();

    // First round is warmup and gets discarded.
    for round in 0..=samples {
        ptrs.clear();

        let t = Instant::now();
        for i in 0..SPLIT_ITERS {
            let b = Box::new(black_box([i as u8; 32]));
            ptrs.push(Box::into_raw(b));
        }
        let per_alloc = t.elapsed().as_nanos() as f64 / SPLIT_ITERS as f64;

        let t = Instant::now();
        for &p in &ptrs {
            drop(unsafe { Box::from_raw(black_box(p)) });
        }
        let per_free = t.elapsed().as_nanos() as f64 / SPLIT_ITERS as f64;

        if round > 0 {
            alloc_ns.push(per_alloc);
            free_ns.push(per_free);
        }
    }

    let stat = |name: &str, note: &str, mut xs: Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Stat {
            name: name.into(),
            median_ns: xs[xs.len() / 2],
            min_ns: xs[0],
            note: note.into(),
        }
    };
    (
        stat(
            "alloc: Box<[u8;32]> malloc only",
            "batched alloc pass, pointers kept",
            alloc_ns,
        ),
        stat(
            "alloc: Box<[u8;32]> free only",
            "batched dealloc pass of the same 1M boxes",
            free_ns,
        ),
    )
}

pub fn run() -> Vec<Stat> {
    let stack = bench(
        "alloc: [u8;32] on the stack",
        "sp bump is free; this is 32 B written to frame",
        7,
        ITERS,
        || {
            for i in 0..ITERS {
                let a = black_box([i as u8; 32]);
                black_box(&a);
            }
        },
    );

    let (heap_alloc, heap_free) = split_alloc_dealloc(7);

    let boxed = bench(
        "alloc: Box<[u8;32]> new + drop (pair)",
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
        "alloc: Vec::with_capacity(32) + drop (pair)",
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

    vec![stack, heap_alloc, heap_free, boxed, vec]
}
