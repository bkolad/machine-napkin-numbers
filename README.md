# machine-napkin-numbers

Measures the classic "latency numbers every programmer should know" on the
machine it runs on. Written for Apple Silicon macOS (buffer sizes are tuned
for M1 Max), but everything except the SLC tier translates to other hardware.

```sh
cargo run --release
```

## What it measures

| # | Benchmark | Technique |
|---|-----------|-----------|
| 1 | `[u8;32]` fetch from L1 / L2 / L3(SLC) / cold RAM | Pointer chase over a randomly-linked cycle: each load address depends on the previous load, defeating pipelining and the prefetcher. Buffer size (16 KiB / 1 MiB / 32 MiB / 512 MiB) selects the tier. |
| 2 | CPU add | Dependent Fibonacci chain (`x+=y; y+=x`) for latency; 4 independent chains for throughput. |
| 3 | `if` predicted vs mispredicted | Same loop over an alternating pattern (always predicted) vs random 50/50 (mispredicted ~half the time); `black_box` in one arm prevents branchless if-conversion. Penalty ≈ 2×delta. |
| 4 | Function call | Dependent chain through a `#[inline(never)]` fn (direct `bl`) and through a black-boxed fn pointer (indirect `blr`). |
| 5 | `[u8;32]` alloc + dealloc | Stack: a local forced to have an address via `black_box` (the sp bump itself is free). Heap split: a batched `Box::into_raw` pass times allocation alone, then a `Box::from_raw` pass times deallocation alone. Heap paired: `Box::new` + drop per iteration; `black_box` on the pointer keeps LLVM from eliding malloc/free. |
| 6 | Thread context switch | Two threads ping-pong over mpsc channels; round trip / 2. |
| 7 | `[u8;32]` from disk | 1 GiB pseudorandom file written with `F_NOCACHE` (never enters page cache). Cold: random 32 B preads through an `F_NOCACHE` fd (real SSD latency). Cached: same reads after warming the page cache (syscall + copy). |

## Sample results (Apple M1 Max, macOS)

```
mem: [u8;32] from L1                              2.1  ns
mem: [u8;32] from L2                              6.5  ns
mem: [u8;32] from L3/SLC                        112    ns
mem: [u8;32] from cold RAM                      135    ns
cpu: add (dependent chain)                        0.33 ns   (~1 cycle @ 3.1 GHz)
cpu: add (independent, throughput)                0.10 ns   (~3 adds/cycle sustained)
branch: if, predicted                             0.32 ns
branch: if, random 50/50                          3.2  ns
branch: mispredict penalty (derived)              5.8  ns   (~18 cycles)
call: direct / fn pointer                         0.97 ns   (~3 cycles)
alloc: [u8;32] on the stack                       0.48 ns   (sp bump is free; 32 B frame write)
alloc: Box<[u8;32]> malloc only                   7.1  ns
alloc: Box<[u8;32]> free only                    11.3  ns
alloc: Box<[u8;32]> new + drop (pair)            16    ns
thread: context switch (ping-pong)             1540    ns
disk: [u8;32] cold read (F_NOCACHE)           99000    ns
disk: [u8;32] cached read (page cache)          590    ns
```

## Caveats

- Apple Silicon has no user-space cycle counter; times are wall-clock
  (`mach_absolute_time`, 41.67 ns ticks) amortized over millions of ops.
  CPU frequency is *estimated* from the add chain assuming 1 cycle/add.
- macOS has no thread-affinity API. The process requests interactive QoS to
  favor P-cores, but the scheduler has the final word.
- Apple's system-level cache (SLC) stands in for a classic L3; the 32 MiB
  working set also incurs TLB misses, so that tier reads high and sits
  close to the cold-RAM number.
- Disk numbers include pread syscall overhead; the cold number is a QD1
  random read of one page — the device's worst case, not its bandwidth.
- Alloc/branch/call numbers include ~1 cycle of arithmetic used to keep the
  dependency chain honest.
