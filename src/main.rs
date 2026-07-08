//! gpu-probe — a GPU matrix-multiply benchmark and measurement instrument.
//!
//! Flow: collect hardware → run benchmark → verify → store row → print summary.
//! Correctness and completeness of the recorded metadata matter more than raw
//! speed: every knob that affects the result is recorded in the row so two runs
//! can be judged comparable (or not) after the fact.

mod benchmark;
mod hardware;
mod store;
mod verify;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

/// All benchmark parameters, every one of which is recorded in the result row.
pub struct Config {
    pub n: u32,
    pub seed: u64,
    pub warmup_iters: u32,
    pub timed_iters: u32,
    pub db_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            n: 1024,                       // multiple of the 16 tile size
            seed: 0x0123_4567_89AB_CDEF,   // fixed default => reproducible
            warmup_iters: 3,
            timed_iters: 20,
            db_path: "./gpu-probe.db".to_string(),
        }
    }
}

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

/// Validate/normalize N: must be a positive multiple of the tile size. If not,
/// round UP to the next multiple of 16 (the tiled shader assumes N % 16 == 0).
fn normalize_n(n: u32) -> u32 {
    let tile = benchmark::TILE_SIZE;
    if n == 0 {
        return tile;
    }
    if n % tile == 0 {
        n
    } else {
        let adjusted = ((n / tile) + 1) * tile;
        println!("Adjusted N from {n} to {adjusted} (must be a multiple of {tile}).");
        adjusted
    }
}

/// UTC timestamp as `YYYY-MM-DDTHH:MM:SSZ` without pulling in chrono. Uses
/// Howard Hinnant's civil-from-days algorithm.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;

    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // days since 1970-01-01 -> civil (y, m, d)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn run() -> Result<bool, String> {
    let mut cfg = parse_args()?;
    cfg.n = normalize_n(cfg.n);

    // Hash the exact shipped-and-run shader text (stable FNV-1a, hex).
    let shader_hash = format!("{:016x}", benchmark::fnv1a_64(benchmark::GEMM_WGSL.as_bytes()));

    // 1. GPU setup (also rejects a CPU/software adapter).
    let ctx = benchmark::setup()?;

    // 2. Host + adapter metadata stamp.
    let hw = hardware::collect(&ctx.adapter_info);

    // 3. Benchmark.
    println!(
        "Running GEMM: N={} seed=0x{:016x} warmup={} timed={} on {} [{}]",
        cfg.n, cfg.seed, cfg.warmup_iters, cfg.timed_iters, hw.gpu_name, hw.backend
    );
    let bench = benchmark::run(&ctx, &cfg);

    // 4. Verify a seeded sample against a CPU recomputation.
    let verification = verify::verify(cfg.n, &bench.a, &bench.b, &bench.c);
    if !verification.passed {
        eprintln!(
            "WARNING: verification FAILED — max relative error {:.3e} exceeds tolerance {:.1e}. \
             Storing row marked failed.",
            verification.max_rel_error,
            verify::REL_TOLERANCE
        );
    }

    // 5. Store the fully-stamped row.
    let timestamp_utc = utc_now();
    let conn = store::open(&cfg.db_path).map_err(|e| format!("failed to open DB: {e}"))?;
    let row_id = store::insert(
        &conn,
        &cfg,
        &hw,
        &bench,
        &verification,
        &shader_hash,
        &timestamp_utc,
    )
    .map_err(|e| format!("failed to insert row: {e}"))?;

    // 6. Human-readable summary.
    println!();
    println!("================ gpu-probe result (row {row_id}) ================");
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
    println!("Shader hash    : {shader_hash}");
    println!("Fingerprint    : {}", bench.input_fingerprint);
    println!("DB             : {} (row {row_id})", cfg.db_path);
    println!("================================================================");

    // Exit non-zero if verification failed or the run was invalid.
    Ok(verification.passed && bench.run_valid)
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
