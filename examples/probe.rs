//! Minimal example of driving the library. Runs one probe with default settings
//! and prints the headline numbers.
//!
//! Run with: `cargo run --example probe`
//!
//! Note: on a machine with no real GPU this exits via `Error::NoGpu` — that is
//! the tool refusing to score a CPU as a GPU, not a bug.

fn main() {
    match gpu_probe::run_probe(&gpu_probe::Config::default()) {
        Ok(outcome) => {
            println!(
                "{} [{}] — median {:.1} GFLOP/s, verification {}",
                outcome.hardware.gpu_name,
                outcome.hardware.backend,
                outcome.benchmark.median_gflops,
                if outcome.verification.passed { "PASS" } else { "FAIL" },
            );
            for w in &outcome.benchmark.warnings {
                eprintln!("warning: {w}");
            }
        }
        Err(e) => {
            eprintln!("probe failed: {e}");
            std::process::exit(1);
        }
    }
}
