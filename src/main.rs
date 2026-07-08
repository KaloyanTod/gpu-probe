//! gpu-probe CLI — a thin front-end over the `gpu_probe` library.
//!
//! Responsibilities kept here: argument parsing and all human-facing I/O
//! (progress, warnings, the result summary, exit code). The measurement logic
//! lives in the library so it can be reused without this shell.

use std::process::ExitCode;

use gpu_probe::{run_probe, util, Config, Error, ProbeOutcome};

const USAGE: &str = "\
gpu-probe — GPU GEMM benchmark → SQLite

USAGE:
    gpu-probe [OPTIONS]

OPTIONS:
    --n <N>            Square matrix size (rounded UP to a multiple of 16). Default 1024.
    --seed <U64>       Deterministic input seed. Default 0x0123456789ABCDEF.
    --warmup <N>       Untimed warm-up iterations. Default 3.
    --iters <N>        Timed iterations. Default 20.
    --db <PATH>        SQLite database path. Default ./gpu-probe.db.
    -h, --help         Print this help.
";

/// Tiny hand-rolled parser (no clap for v1). Each flag takes one value.
fn parse_args() -> Result<Config, String> {
    let mut cfg = Config::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        let mut next = |name: &str| {
            args.next()
                .ok_or_else(|| format!("missing value for {name}"))
        };
        match arg.as_str() {
            "--n" => {
                cfg.n = next("--n")?
                    .parse()
                    .map_err(|_| "invalid --n".to_string())?;
            }
            "--seed" => {
                let s = next("--seed")?;
                cfg.seed = parse_u64(&s).ok_or_else(|| "invalid --seed".to_string())?;
            }
            "--warmup" => {
                cfg.warmup_iters = next("--warmup")?
                    .parse()
                    .map_err(|_| "invalid --warmup".to_string())?;
            }
            "--iters" => {
                cfg.timed_iters = next("--iters")?
                    .parse()
                    .map_err(|_| "invalid --iters".to_string())?;
            }
            "--db" => {
                cfg.db_path = next("--db")?;
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(cfg)
}

/// Parse a u64 accepting both decimal and 0x-hex forms.
fn parse_u64(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// Print everything the run produced, mirroring the library's collected output
/// (enumerated adapters, warnings) plus a human-readable summary.
fn report(outcome: &ProbeOutcome) {
    let hw = &outcome.hardware;
    let bench = &outcome.benchmark;
    let cfg = &outcome.config;
    let verification = &outcome.verification;

    // Adapter enumeration (previously printed inside setup()).
    eprintln!("Enumerated {} adapter(s):", outcome.enumerated_adapters.len());
    for line in &outcome.enumerated_adapters {
        eprintln!("  - {line}");
    }

    println!(
        "Running GEMM: N={} seed=0x{:016x} warmup={} timed={} on {} [{}]",
        cfg.n, cfg.seed, cfg.warmup_iters, cfg.timed_iters, hw.gpu_name, hw.backend
    );

    // Warnings collected during the timed run.
    for w in &bench.warnings {
        eprintln!("warning: {w}");
    }
    if !verification.passed {
        eprintln!(
            "WARNING: verification FAILED — max relative error {:.3e} exceeds tolerance {:.1e}. \
             Storing row marked failed.",
            verification.max_rel_error,
            gpu_probe::verify::REL_TOLERANCE
        );
    }

    println!();
    let row_label = row_label(outcome);
    println!("================ gpu-probe result{row_label} ================");
    println!("GPU            : {} [{}]", hw.gpu_name, hw.gpu_device_type);
    println!("Backend        : {}", hw.backend);
    println!("Timing method  : {}", bench.timing_method);
    println!("Matrix N       : {}", cfg.n);
    println!(
        "Median         : {:.3} ms  ({:.1} GFLOP/s)",
        bench.median_ms, bench.median_gflops
    );
    println!(
        "Min (fastest)  : {:.3} ms  ({:.1} GFLOP/s)",
        bench.min_ms, bench.min_gflops
    );
    println!(
        "Verification   : {} (max rel err {:.3e})",
        if verification.passed { "PASS" } else { "FAIL" },
        verification.max_rel_error
    );
    println!("Run valid      : {}", bench.run_valid);
    println!("Shader hash    : {}", outcome.shader_hash);
    println!("Fingerprint    : {}", bench.input_fingerprint);
    #[cfg(feature = "sqlite")]
    if let Some(id) = outcome.row_id {
        println!("DB             : {} (row {id})", cfg.db_path);
    }
    println!("================================================================");
}

#[cfg(feature = "sqlite")]
fn row_label(outcome: &ProbeOutcome) -> String {
    match outcome.row_id {
        Some(id) => format!(" (row {id})"),
        None => String::new(),
    }
}

#[cfg(not(feature = "sqlite"))]
fn row_label(_outcome: &ProbeOutcome) -> String {
    String::new()
}

fn run() -> Result<bool, Error> {
    let mut cfg = parse_args().map_err(Error::Device)?;

    // Normalize here so we can tell the user we adjusted their N; run_probe
    // normalizes again idempotently.
    let normalized = util::normalize_n(cfg.n);
    if normalized != cfg.n {
        println!(
            "Adjusted N from {} to {normalized} (must be a multiple of 16).",
            cfg.n
        );
        cfg.n = normalized;
    }

    let outcome = run_probe(&cfg)?;
    report(&outcome);

    // Exit non-zero if verification failed or the run was invalid.
    Ok(outcome.verification.passed && outcome.benchmark.run_valid)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("gpu-probe: run stored but flagged (verification failed or run invalid).");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("gpu-probe: {e}");
            ExitCode::FAILURE
        }
    }
}
