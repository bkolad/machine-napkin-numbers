//! Benchmark 2: CPU integer add.
//!
//! Latency: a Fibonacci-style dependent chain (x += y; y += x). Every add
//! needs the previous result, so the chain runs at true add latency
//! (1 cycle on every modern core). The loop counter runs in parallel on
//! spare ports, so loop overhead is hidden by the chain.
//!
//! Throughput: four independent chains expose the number of ALU ports.

use crate::harness::{bench, Stat};
use std::hint::black_box;

pub fn run() -> Vec<Stat> {
    const ITERS: u64 = 8_000_000;
    const ADDS_PER_ITER: u64 = 8;

    let latency = bench(
        "cpu: add (dependent chain)",
        "true add latency, ~1 cycle",
        7,
        ITERS * ADDS_PER_ITER,
        || {
            let mut x = black_box(1u64);
            let mut y = black_box(3u64);
            for _ in 0..ITERS {
                x = x.wrapping_add(y);
                y = y.wrapping_add(x);
                x = x.wrapping_add(y);
                y = y.wrapping_add(x);
                x = x.wrapping_add(y);
                y = y.wrapping_add(x);
                x = x.wrapping_add(y);
                y = y.wrapping_add(x);
            }
            black_box((x, y));
        },
    );

    let throughput = bench(
        "cpu: add (independent, throughput)",
        "4 independent chains, shows ALU port count",
        7,
        ITERS * ADDS_PER_ITER,
        || {
            let mut a = black_box(1u64);
            let mut b = black_box(2u64);
            let mut c = black_box(3u64);
            let mut d = black_box(4u64);
            let mut e = black_box(5u64);
            let mut f = black_box(6u64);
            let mut g = black_box(7u64);
            let mut h = black_box(8u64);
            for _ in 0..ITERS {
                a = a.wrapping_add(b);
                b = b.wrapping_add(a);
                c = c.wrapping_add(d);
                d = d.wrapping_add(c);
                e = e.wrapping_add(f);
                f = f.wrapping_add(e);
                g = g.wrapping_add(h);
                h = h.wrapping_add(g);
            }
            black_box((a, b, c, d, e, f, g, h));
        },
    );

    vec![latency, throughput]
}

/// Estimated CPU frequency, assuming the dependent add chain retires one
/// add per cycle (true on all modern cores). Apple Silicon exposes no
/// user-space cycle counter, so this is how we get an approximate GHz.
pub fn estimated_ghz(add_latency_ns: f64) -> f64 {
    1.0 / add_latency_ns
}
