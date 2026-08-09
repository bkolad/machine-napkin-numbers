//! Benchmark 7: reading a [u8; 32] from disk.
//!
//! A 1 GiB file of pseudorandom data is written with F_NOCACHE set (macOS's
//! O_DIRECT equivalent) so its pages never enter the page cache. Then:
//!   - cold:   32-byte preads at random page-aligned offsets through an
//!             F_NOCACHE fd -> real SSD latency per read.
//!   - cached: the file is warmed into the page cache through a normal fd,
//!             then the same random preads -> syscall + memcpy cost.
//! Cold runs first so warming can't pollute it.

use crate::harness::{bench, Stat};
use rand::Rng;
use std::fs::{self, File, OpenOptions};
use std::hint::black_box;
use std::io::{Read, Write};
use std::os::unix::fs::FileExt;
use std::os::unix::io::AsRawFd;

const FILE_SIZE: u64 = 1 << 30; // 1 GiB
const BLOCK: usize = 4 << 20; // 4 MiB write chunks

fn set_nocache(f: &File, on: bool) {
    let r = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, on as libc::c_int) };
    assert_ne!(r, -1, "fcntl(F_NOCACHE) failed");
}

fn random_page_offsets(count: usize) -> Vec<u64> {
    let mut rng = rand::thread_rng();
    let pages = FILE_SIZE / 4096;
    (0..count)
        .map(|_| rng.gen_range(0..pages) * 4096)
        .collect()
}

pub fn run() -> Vec<Stat> {
    let path = std::env::temp_dir().join("machine_napkin_numbers_disk.bin");
    eprintln!("  writing 1 GiB test file (uncached) to {} ...", path.display());

    {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .expect("create test file");
        set_nocache(&f, true); // keep the written pages out of the page cache

        // Pseudorandom block so APFS compression can't shrink the file.
        let mut block = vec![0u8; BLOCK];
        rand::thread_rng().fill(&mut block[..]);
        let mut written = 0u64;
        while written < FILE_SIZE {
            block[..8].copy_from_slice(&written.to_le_bytes());
            f.write_all(&block).expect("write test file");
            written += BLOCK as u64;
        }
        f.sync_all().expect("sync");
    }

    // --- cold reads ---
    const COLD_READS: usize = 2_000;
    let cold_file = File::open(&path).expect("open");
    set_nocache(&cold_file, true);
    let offsets = random_page_offsets(COLD_READS);
    let cold = bench(
        "disk: [u8;32] cold read (F_NOCACHE)",
        "random 32 B preads, hits the SSD",
        5,
        COLD_READS as u64,
        || {
            let mut buf = [0u8; 32];
            for &off in &offsets {
                cold_file.read_exact_at(&mut buf, off).expect("pread");
                black_box(&buf);
            }
        },
    );

    // --- cached reads ---
    eprintln!("  warming file into page cache ...");
    let cached_file = File::open(&path).expect("open");
    {
        let mut warm = File::open(&path).expect("open");
        let mut buf = vec![0u8; 8 << 20];
        loop {
            let n = warm.read(&mut buf).expect("warm read");
            if n == 0 {
                break;
            }
            black_box(&buf[0]);
        }
    }
    const CACHED_READS: usize = 100_000;
    let offsets = random_page_offsets(CACHED_READS);
    let cached = bench(
        "disk: [u8;32] cached read (page cache)",
        "pread syscall + copy, no device I/O",
        5,
        CACHED_READS as u64,
        || {
            let mut buf = [0u8; 32];
            for &off in &offsets {
                cached_file.read_exact_at(&mut buf, off).expect("pread");
                black_box(&buf);
            }
        },
    );

    drop(cold_file);
    drop(cached_file);
    let _ = fs::remove_file(&path);

    vec![cold, cached]
}
