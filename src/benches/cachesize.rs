//! Empirical cache-size detection.
//!
//! Runs the same random pointer chase as the memory benchmark over a sweep
//! of working-set sizes. Latency is flat while the set fits in a cache
//! tier and jumps when it spills into the next one, so the jump locations
//! are the cache capacities — measured, not queried from the OS.
//!
//! Boundaries are fuzzy by nature (associativity, TLB reach, shared/
//! victim caches all smear the edges), so results are estimates: the last
//! size whose latency still belonged to the lower plateau.

use crate::benches::memory::{chase, make_cycle};
use std::hint::black_box;
use std::time::Instant;

pub struct SweepPoint {
    pub bytes: usize,
    pub ns: f64,
}

pub struct SweepResult {
    pub points: Vec<SweepPoint>,
    /// (tier name, last size in bytes that still fit)
    pub boundaries: Vec<(String, usize)>,
}

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

pub fn run() -> SweepResult {
    let sizes: &[usize] = &[
        8 * KIB,
        16 * KIB,
        32 * KIB,
        48 * KIB,
        64 * KIB,
        96 * KIB,
        128 * KIB,
        192 * KIB,
        256 * KIB,
        384 * KIB,
        512 * KIB,
        768 * KIB,
        MIB,
        3 * MIB / 2,
        2 * MIB,
        3 * MIB,
        4 * MIB,
        6 * MIB,
        8 * MIB,
        12 * MIB,
        16 * MIB,
        24 * MIB,
        32 * MIB,
        48 * MIB,
        64 * MIB,
        96 * MIB,
        128 * MIB,
    ];

    let mut points = Vec::with_capacity(sizes.len());
    for &bytes in sizes {
        let nodes = make_cycle(bytes);
        let steps: u64 = if bytes <= MIB {
            1_000_000
        } else if bytes <= 16 * MIB {
            500_000
        } else {
            250_000
        };

        black_box(chase(&nodes, steps)); // warmup lap
        let mut best = f64::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            black_box(chase(&nodes, steps));
            best = best.min(t.elapsed().as_nanos() as f64 / steps as f64);
        }
        eprintln!("    {:>9} {:>8.2} ns", human(bytes), best);
        points.push(SweepPoint { bytes, ns: best });
    }

    let boundaries = detect_boundaries(&points);
    SweepResult { points, boundaries }
}

/// A tier boundary shows up as a run of consecutive latency increases.
/// Group adjacent jumps (ratio > threshold) into one boundary and report
/// the size just before the group started.
fn detect_boundaries(points: &[SweepPoint]) -> Vec<(String, usize)> {
    const JUMP: f64 = 1.35;
    let mut edges: Vec<usize> = Vec::new(); // index of last point before a jump group
    let mut in_group = false;
    for i in 0..points.len() - 1 {
        if points[i + 1].ns > points[i].ns * JUMP {
            if !in_group {
                edges.push(i);
                in_group = true;
            }
        } else {
            in_group = false;
        }
    }

    let names = ["L1", "L2", "L3/SLC", "tier4"];
    edges
        .into_iter()
        .take(names.len())
        .enumerate()
        .map(|(n, i)| (names[n].to_string(), points[i].bytes))
        .collect()
}

pub fn human(bytes: usize) -> String {
    if bytes >= MIB && bytes % MIB == 0 {
        format!("{} MiB", bytes / MIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{} KiB", bytes / KIB)
    }
}
