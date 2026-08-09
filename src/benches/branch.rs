//! Benchmark 3: `if` statement — predicted vs. mispredicted.
//!
//! The exact same loop runs over two inputs: an alternating 0/1 pattern
//! (trivially predicted by any modern branch predictor) and a random
//! 50/50 pattern (mispredicted ~half the time). The difference isolates
//! the misprediction penalty. `black_box` in one arm stops the compiler
//! from if-converting the branch into a branchless csel/cmov.

use crate::harness::{bench, Stat};
use rand::Rng;
use std::hint::black_box;

const LEN: usize = 65_536;
const REPEATS: u64 = 256;

fn run_branches(data: &[u8]) -> u64 {
    let mut acc = 0u64;
    for _ in 0..REPEATS {
        for &b in data {
            if b == 1 {
                acc = acc.wrapping_add(black_box(1));
            } else {
                acc = acc.rotate_left(1);
            }
        }
    }
    acc
}

pub fn run() -> Vec<Stat> {
    let mut rng = rand::thread_rng();
    let predictable: Vec<u8> = (0..LEN).map(|i| (i & 1) as u8).collect();
    let random: Vec<u8> = (0..LEN).map(|_| rng.gen::<bool>() as u8).collect();
    let branches = LEN as u64 * REPEATS;

    let hit = bench(
        "branch: if, predicted (alternating)",
        "predictor hits ~100%",
        5,
        branches,
        || {
            black_box(run_branches(&predictable));
        },
    );
    let miss = bench(
        "branch: if, random 50/50",
        "predictor misses ~50% of branches",
        5,
        branches,
        || {
            black_box(run_branches(&random));
        },
    );

    // Random data mispredicts ~50% of the time, so the per-mispredict
    // penalty is roughly twice the per-branch delta.
    let penalty = Stat {
        name: "branch: mispredict penalty (derived)".into(),
        median_ns: (miss.median_ns - hit.median_ns) * 2.0,
        min_ns: (miss.min_ns - hit.min_ns) * 2.0,
        note: "2 x (random - predicted), cost of one pipeline flush".into(),
    };

    vec![hit, miss, penalty]
}
