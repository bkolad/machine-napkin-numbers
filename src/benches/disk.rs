//! Benchmark 7: reading a [u8; 32] from disk.
//!
//! A 1 GiB file of pseudorandom data is written and evicted from the page
//! cache (macOS: F_NOCACHE on the writing fd; Linux: posix_fadvise DONTNEED
//! after sync). Then:
//!   - cold:   reads at random page-aligned offsets through an uncached fd
//!             (macOS: 32 B preads with F_NOCACHE; Linux: 4 KiB O_DIRECT
//!             preads, since O_DIRECT requires block-aligned lengths)
//!             -> real SSD latency per read.
//!   - cached: the file is warmed into the page cache through a normal fd,
//!             then random 32 B preads -> syscall + memcpy cost.
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

#[cfg(target_os = "macos")]
fn set_nocache(f: &File, on: bool) {
    let r = unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, on as libc::c_int) };
    assert_ne!(r, -1, "fcntl(F_NOCACHE) failed");
}

#[cfg(target_os = "linux")]
fn drop_cache(f: &File) {
    let r = unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    assert_eq!(r, 0, "posix_fadvise(POSIX_FADV_DONTNEED) failed");
}

/// Page-aligned heap buffer, required for O_DIRECT reads on Linux.
#[cfg(target_os = "linux")]
struct AlignedBuf {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

#[cfg(target_os = "linux")]
impl AlignedBuf {
    fn new(len: usize) -> Self {
        let layout = std::alloc::Layout::from_size_align(len, 4096).unwrap();
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "aligned alloc failed");
        AlignedBuf { ptr, layout }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

#[cfg(target_os = "linux")]
impl Drop for AlignedBuf {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr, self.layout) }
    }
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
        #[cfg(target_os = "macos")]
        set_nocache(&f, true); // keep the written pages out of the page cache

        // Pseudorandom block so filesystem compression can't shrink the file.
        let mut block = vec![0u8; BLOCK];
        rand::thread_rng().fill(&mut block[..]);
        let mut written = 0u64;
        while written < FILE_SIZE {
            block[..8].copy_from_slice(&written.to_le_bytes());
            f.write_all(&block).expect("write test file");
            written += BLOCK as u64;
        }
        f.sync_all().expect("sync");
        #[cfg(target_os = "linux")]
        drop_cache(&f); // evict the written pages from the page cache
    }

    // --- cold reads ---
    const COLD_READS: usize = 2_000;
    let offsets = random_page_offsets(COLD_READS);

    #[cfg(target_os = "macos")]
    let cold = {
        let cold_file = File::open(&path).expect("open");
        set_nocache(&cold_file, true);
        bench(
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
        )
    };

    #[cfg(target_os = "linux")]
    let cold = {
        use std::os::unix::fs::OpenOptionsExt;
        let cold_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(&path)
            .expect("open O_DIRECT");
        let mut buf = AlignedBuf::new(4096);
        bench(
            "disk: [u8;32] cold read (O_DIRECT)",
            "random 4 KiB preads, hits the SSD",
            5,
            COLD_READS as u64,
            || {
                let page = buf.as_mut_slice();
                for &off in &offsets {
                    cold_file.read_exact_at(page, off).expect("pread");
                    black_box(&page[..32]);
                }
            },
        )
    };

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

    drop(cached_file);
    let _ = fs::remove_file(&path);

    vec![cold, cached]
}
