use std::time::Instant;

pub struct Stat {
    pub name: String,
    pub median_ns: f64,
    pub min_ns: f64,
    pub note: String,
}

/// Runs `f` once as warmup, then `samples` more times. `f` must perform
/// `ops_per_iter` operations of the thing being measured; the reported
/// numbers are wall-clock time divided by that count.
///
/// Median over samples filters out scheduler noise; min is the best case
/// (closest to the true hardware cost for tight loops).
pub fn bench(
    name: &str,
    note: &str,
    samples: usize,
    ops_per_iter: u64,
    mut f: impl FnMut(),
) -> Stat {
    f(); // warmup

    let mut per_op = Vec::with_capacity(samples);
    for _ in 0..samples {
        let t = Instant::now();
        f();
        let dt = t.elapsed().as_nanos() as f64;
        per_op.push(dt / ops_per_iter as f64);
    }
    per_op.sort_by(|a, b| a.partial_cmp(b).unwrap());

    Stat {
        name: name.to_string(),
        median_ns: per_op[per_op.len() / 2],
        min_ns: per_op[0],
        note: note.to_string(),
    }
}

pub fn print_table(stats: &[Stat]) {
    println!();
    println!(
        "{:<46} {:>13} {:>13}   {}",
        "benchmark", "median ns/op", "min ns/op", "note"
    );
    println!("{}", "-".repeat(120));
    for s in stats {
        println!(
            "{:<46} {:>13} {:>13}   {}",
            s.name,
            fmt_ns(s.median_ns),
            fmt_ns(s.min_ns),
            s.note
        );
    }
}

fn fmt_ns(ns: f64) -> String {
    if ns < 10.0 {
        format!("{:.3}", ns)
    } else if ns < 1000.0 {
        format!("{:.1}", ns)
    } else {
        format!("{:.0}", ns)
    }
}
