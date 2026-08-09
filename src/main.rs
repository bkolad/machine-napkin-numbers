//! machine-napkin-numbers: measures the classic "latency numbers every
//! programmer should know" on this machine.
//!
//! Run with: cargo run --release

mod benches;
mod harness;

// Ask the macOS scheduler for interactive QoS so we land on P-cores.
// (Declared by hand; value is QOS_CLASS_USER_INTERACTIVE = 0x21.)
#[cfg(target_os = "macos")]
extern "C" {
    fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
}

#[cfg(target_os = "macos")]
fn boost_priority() {
    unsafe {
        pthread_set_qos_class_self_np(0x21, 0);
    }
}

#[cfg(target_os = "linux")]
fn boost_priority() {
    // Best effort; silently ignored without CAP_SYS_NICE.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, -20);
    }
}

#[cfg(target_os = "macos")]
fn sysctl(key: &str) -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .arg("-n")
        .arg(key)
        .output()
        .ok()?;
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !val.is_empty()).then_some(val)
}

fn human_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{} MiB", b >> 20)
    } else if b >= 1 << 10 {
        format!("{} KiB", b >> 10)
    } else {
        format!("{} B", b)
    }
}

#[cfg(target_os = "macos")]
fn print_machine_info() {
    println!("machine");
    println!("{}", "-".repeat(60));

    let human = |v: &str| v.parse::<u64>().map(human_bytes).unwrap_or_else(|_| v.to_string());

    // (label, sysctl key, format as bytes?)
    let rows: [(&str, &str, bool); 15] = [
        ("CPU", "machdep.cpu.brand_string", false),
        ("macOS", "kern.osproductversion", false),
        ("logical cores", "hw.ncpu", false),
        ("P-cores", "hw.perflevel0.physicalcpu", false),
        ("E-cores", "hw.perflevel1.physicalcpu", false),
        ("P-core L1d", "hw.perflevel0.l1dcachesize", true),
        ("P-core L1i", "hw.perflevel0.l1icachesize", true),
        ("P-cluster L2", "hw.perflevel0.l2cachesize", true),
        ("cores per P-cluster L2", "hw.perflevel0.cpusperl2", false),
        ("E-core L1d", "hw.perflevel1.l1dcachesize", true),
        ("E-cluster L2", "hw.perflevel1.l2cachesize", true),
        ("cache line", "hw.cachelinesize", false),
        ("page size", "hw.pagesize", true),
        ("RAM", "hw.memsize", true),
        ("timer tick (tbfrequency)", "hw.tbfrequency", false),
    ];
    for (label, key, as_bytes) in rows {
        if let Some(val) = sysctl(key) {
            let shown = if as_bytes { human(&val) } else { val };
            println!("{:<26} {}", label, shown);
        }
    }
    println!("{:<26} not exposed by sysctl (48 MiB on M1 Max)", "SLC (\"L3\")");
}

#[cfg(target_os = "linux")]
fn print_machine_info() {
    use std::fs;

    println!("machine");
    println!("{}", "-".repeat(60));

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        if let Some(model) = cpuinfo
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
        {
            println!("{:<26} {}", "CPU", model.trim());
        }
    }
    if let Ok(rel) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        println!("{:<26} {}", "kernel", rel.trim());
    }
    if let Ok(n) = std::thread::available_parallelism() {
        println!("{:<26} {}", "logical cores", n);
    }

    // Caches for cpu0 from sysfs. "size" is like "32K"/"1024K"/"36864K".
    let cache_dir = "/sys/devices/system/cpu/cpu0/cache";
    let read = |p: String| fs::read_to_string(p).ok().map(|s| s.trim().to_string());
    for i in 0..6 {
        let level = read(format!("{cache_dir}/index{i}/level"));
        let typ = read(format!("{cache_dir}/index{i}/type"));
        let size = read(format!("{cache_dir}/index{i}/size"));
        if let (Some(level), Some(typ), Some(size)) = (level, typ, size) {
            let label = match (level.as_str(), typ.as_str()) {
                ("1", "Data") => "L1d".to_string(),
                ("1", "Instruction") => "L1i".to_string(),
                (l, _) => format!("L{l}"),
            };
            println!("{:<26} {}", label, size);
        }
    }
    if let Some(line) = read(format!("{cache_dir}/index0/coherency_line_size")) {
        println!("{:<26} {}", "cache line", line);
    }
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page > 0 {
        println!("{:<26} {}", "page size", human_bytes(page as u64));
    }
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        if let Some(kb) = meminfo
            .lines()
            .find(|l| l.starts_with("MemTotal"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
        {
            println!("{:<26} {}", "RAM", human_bytes(kb * 1024));
        }
    }
}

fn main() {
    boost_priority();

    println!("machine-napkin-numbers: hardware latency micro-benchmarks");
    println!("{}", "=".repeat(60));

    let mut stats = Vec::new();

    eprintln!("[1/7] memory hierarchy (pointer chase) ...");
    stats.extend(benches::memory::run());

    eprintln!("[2/7] cpu add ...");
    let alu = benches::alu::run();
    let ghz = benches::alu::estimated_ghz(alu[0].median_ns);
    stats.extend(alu);

    eprintln!("[3/7] branches ...");
    stats.extend(benches::branch::run());

    eprintln!("[4/7] function calls ...");
    stats.extend(benches::call::run());

    eprintln!("[5/7] allocation ...");
    stats.extend(benches::alloc::run());

    eprintln!("[6/7] thread context switch ...");
    stats.extend(benches::thread::run());

    eprintln!("[7/7] disk reads ...");
    stats.extend(benches::disk::run());

    println!();
    print_machine_info();
    println!(
        "{:<26} {:.2} GHz (est. from add-chain latency, 1 cycle/add)",
        "CPU frequency", ghz
    );

    harness::print_table(&stats);

    println!();
    #[cfg(target_os = "macos")]
    {
        println!("caveats: wall-clock timer (no user-space cycle counter on Apple Silicon),");
        println!("no thread pinning on macOS, SLC stands in for a classic L3.");
    }
    #[cfg(target_os = "linux")]
    {
        println!("caveats: wall-clock timer, no thread pinning; frequency scaling");
        println!("and scheduler migration add noise.");
    }
}
