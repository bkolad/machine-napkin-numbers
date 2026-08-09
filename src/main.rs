//! machine-napkin-numbers: measures the classic "latency numbers every
//! programmer should know" on this machine.
//!
//! Run with: cargo run --release

mod benches;
mod harness;

use std::process::Command;

// Ask the macOS scheduler for interactive QoS so we land on P-cores.
// (Declared by hand; value is QOS_CLASS_USER_INTERACTIVE = 0x21.)
extern "C" {
    fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
}

fn sysctl(key: &str) -> Option<String> {
    let out = Command::new("sysctl").arg("-n").arg(key).output().ok()?;
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !val.is_empty()).then_some(val)
}

fn human_bytes(v: &str) -> String {
    match v.parse::<u64>() {
        Ok(b) if b >= 1 << 30 => format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64),
        Ok(b) if b >= 1 << 20 => format!("{} MiB", b >> 20),
        Ok(b) if b >= 1 << 10 => format!("{} KiB", b >> 10),
        _ => v.to_string(),
    }
}

fn print_machine_info() {
    println!("machine");
    println!("{}", "-".repeat(60));

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
            let shown = if as_bytes { human_bytes(&val) } else { val };
            println!("{:<26} {}", label, shown);
        }
    }
    println!("{:<26} not exposed by sysctl (48 MiB on M1 Max)", "SLC (\"L3\")");
}

fn main() {
    unsafe {
        pthread_set_qos_class_self_np(0x21, 0);
    }

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
    println!("caveats: wall-clock timer (no user-space cycle counter on Apple Silicon),");
    println!("no thread pinning on macOS, SLC stands in for a classic L3.");
}
