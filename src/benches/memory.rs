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

/// Sequential sum over the whole buffer with independent accumulators, so
/// loads pipeline and the prefetcher streams — measures single-core DRAM
/// bandwidth rather than latency. LLVM vectorizes this to NEON/SIMD loads.
fn stream_read(buf: &[u64]) -> u64 {
    // Four independent sequential streams over the buffer's quarters keep
    // more misses in flight than one stream the prefetcher must chase.
    let q = buf.len() / 4;
    let (s0, rest) = buf.split_at(q);
    let (s1, rest) = rest.split_at(q);
    let (s2, s3) = rest.split_at(q);
    let mut acc = [0u64; 16];
    for (((c0, c1), c2), c3) in s0
        .chunks_exact(4)
        .zip(s1.chunks_exact(4))
        .zip(s2.chunks_exact(4))
        .zip(s3.chunks_exact(4))
    {
        for i in 0..4 {
            acc[i] = acc[i].wrapping_add(c0[i]);
            acc[4 + i] = acc[4 + i].wrapping_add(c1[i]);
            acc[8 + i] = acc[8 + i].wrapping_add(c2[i]);
            acc[12 + i] = acc[12 + i].wrapping_add(c3[i]);
        }
    }
    acc.iter().fold(0, |a, b| a ^ b)
}

fn bandwidth() -> Stat {
    const BYTES: usize = 1 << 30; // 1 GiB, far beyond every cache
    eprintln!("  building 1 GiB streaming buffer ...");
    // Nonzero fill: untouched zero pages would all alias the shared zero
    // page and be served from cache instead of DRAM.
    let buf: Vec<u64> = (0..BYTES / 8).map(|i| i as u64).collect();

    let chunks = (BYTES / 32) as u64; // report per [u8; 32] fetched
    let mut stat = bench(
        "mem: [u8;32] sequential stream (1 core)",
        "",
        5,
        chunks,
        || {
            black_box(stream_read(&buf));
        },
    );
    // 1 byte per ns == 1 GB/s, so GB/s = 32 / (ns per 32 B chunk).
    stat.note = format!(
        "single-core DRAM bandwidth ~{:.0} GB/s, 1 GiB sweep",
        32.0 / stat.median_ns
    );
    stat
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
    stats.push(bandwidth());
    stats
}
