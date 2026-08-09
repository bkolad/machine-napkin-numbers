//! Benchmark 1: fetching a [u8; 32] from L1 / L2 / L3(SLC) / cold RAM.
//!
//! Technique: pointer chasing over a randomly-linked cycle of nodes.
//! Each load's address depends on the previous load's result, so the CPU
//! cannot overlap or pipeline the accesses, and the random order defeats
//! the hardware prefetcher. Buffer size selects which cache tier the
//! working set lives in.

use crate::harness::{bench, Stat};
use rand::seq::SliceRandom;
use std::hint::black_box;

// One node per cache line. Apple Silicon lines are 128 bytes; using the
// full line as the stride also avoids adjacent-line prefetch effects.
#[repr(C, align(128))]
#[derive(Clone)]
struct Node {
    next: u64,
    payload: [u8; 32],
}

fn make_cycle(bytes: usize) -> Vec<Node> {
    let n = bytes / std::mem::size_of::<Node>();
    assert!(n >= 2);
    let mut nodes = vec![
        Node {
            next: 0,
            payload: [0; 32],
        };
        n
    ];

    // Random permutation linked into a single Hamiltonian cycle.
    let mut order: Vec<u64> = (0..n as u64).collect();
    order.shuffle(&mut rand::thread_rng());
    for i in 0..n {
        let from = order[i] as usize;
        nodes[from].next = order[(i + 1) % n];
        nodes[from].payload = [from as u8; 32];
    }
    nodes
}

/// Follow the chain for `steps` hops, reading the full 32-byte payload
/// at every hop. Returns an accumulator so nothing can be optimized out.
fn chase(nodes: &[Node], steps: u64) -> u64 {
    let mut idx = 0u64;
    let mut acc = 0u64;
    for _ in 0..steps {
        // Safety: `next` is always a valid index into `nodes`.
        let node = unsafe { nodes.get_unchecked(idx as usize) };
        // Read all 32 payload bytes (4x u64, 8-byte aligned within the node).
        let p = node.payload.as_ptr() as *const u64;
        acc ^= unsafe { *p ^ *p.add(1) ^ *p.add(2) ^ *p.add(3) };
        idx = node.next; // the dependent load that sets the pace
    }
    black_box(acc);
    idx
}

pub fn run() -> Vec<Stat> {
    // Sized for Apple M1 Max: L1d 128 KiB (P-core), L2 12 MiB (P-cluster),
    // SLC ("L3") 48 MiB. Small enough / large enough to also land in the
    // right tier on most x86 parts, except SLC which is Apple-specific.
    let tiers: [(&str, usize, u64, &str); 4] = [
        (
            "mem: [u8;32] from L1",
            16 << 10,
            8_000_000,
            "16 KiB working set, dependent random loads",
        ),
        (
            "mem: [u8;32] from L2",
            1 << 20,
            4_000_000,
            "1 MiB working set",
        ),
        (
            "mem: [u8;32] from L3/SLC",
            32 << 20,
            2_000_000,
            "32 MiB working set (Apple system-level cache)",
        ),
        (
            "mem: [u8;32] from cold RAM",
            512 << 20,
            1_000_000,
            "512 MiB working set, incl. TLB misses",
        ),
    ];

    let mut stats = Vec::new();
    for (name, bytes, steps, note) in tiers {
        eprintln!("  building {} chase buffer ...", name);
        let nodes = make_cycle(bytes);
        stats.push(bench(name, note, 5, steps, || {
            black_box(chase(&nodes, steps));
        }));
    }
    stats
}
