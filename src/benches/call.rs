//! Benchmark 4: function call.
//!
//! A dependent chain of calls (each call's argument is the previous
//! result) so calls cannot overlap. Direct call: statically-known target,
//! `bl`/`ret`. Indirect call: through a black-boxed function pointer,
//! `blr` — what dyn Trait / callbacks cost. Each call body is a single
//! add, so the numbers include ~1 cycle of work.

use crate::harness::{bench, Stat};
use std::hint::black_box;

#[inline(never)]
fn callee(x: u64) -> u64 {
    x.wrapping_add(1)
}

pub fn run() -> Vec<Stat> {
    const ITERS: u64 = 16_000_000;

    let direct = bench(
        "call: direct #[inline(never)] fn",
        "bl/ret + 1 add, dependent chain",
        7,
        ITERS,
        || {
            let mut x = black_box(0u64);
            for _ in 0..ITERS {
                x = callee(x);
            }
            black_box(x);
        },
    );

    let indirect = bench(
        "call: through fn pointer",
        "indirect blr, like dyn Trait/callback",
        7,
        ITERS,
        || {
            let f: fn(u64) -> u64 = black_box(callee as fn(u64) -> u64);
            let mut x = black_box(0u64);
            for _ in 0..ITERS {
                x = f(x);
            }
            black_box(x);
        },
    );

    vec![direct, indirect]
}
