//! gpu-probe — a reproducible GPU matrix-multiply (GEMM) benchmark and
//! measurement instrument.
//!
//! It runs a tiled f32 GEMM (`C = A * B`) on a real GPU via [`wgpu`], collects a
//! full hardware/software stamp, verifies the result against a CPU recomputation,
//! and (with the `sqlite` feature) records one fully-stamped row per run.
//!
//! Correctness and completeness of the recorded metadata matter more than raw
//! speed: every knob that affects the number is recorded so two runs can be
//! judged comparable — or not — after the fact.
//!
//! # Quick start
//!
//! ```no_run
//! use gpu_probe::{Config, run_probe};
//!
//! let outcome = run_probe(&Config::default())?;
//! println!("{:.1} GFLOP/s", outcome.benchmark.median_gflops);
//! # Ok::<(), gpu_probe::Error>(())
//! ```
//!
//! [`run_probe`] is the one-call convenience path. The building blocks it composes
//! ([`benchmark::setup`], [`hardware::collect`], [`benchmark::run`],
//! [`verify::verify`], and the [`store`] module) are all public if you need finer
//! control.

pub mod benchmark;
pub mod error;
pub mod hardware;
#[cfg(feature = "sqlite")]
pub mod store;
pub mod util;
pub mod verify;

pub use benchmark::{BenchmarkResult, GpuContext};
pub use error::Error;
pub use hardware::HardwareInfo;
pub use verify::VerifyResult;

/// All benchmark parameters, every one of which is recorded in the result row.
#[derive(Debug, Clone)]
pub struct Config {
    /// Square matrix dimension. Rounded up to a multiple of [`benchmark::TILE_SIZE`].
    pub n: u32,
    /// Deterministic input seed — the same seed yields bit-identical matrices
    /// on every machine.
    pub seed: u64,
    /// Untimed warm-up iterations run before timing begins.
    pub warmup_iters: u32,
    /// Timed iterations that contribute to the reported statistics.
    pub timed_iters: u32,
    /// SQLite database path (used only with the `sqlite` feature).
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

/// Everything a single probe produced: the (normalized) config, the full
/// hardware stamp, the benchmark statistics, the verification result, the shader
/// hash, and the UTC timestamp. With the `sqlite` feature it also carries the
/// stored row id. A consumer renders its own summary from these fields.
pub struct ProbeOutcome {
    /// The effective config actually used (with `n` normalized).
    pub config: Config,
    /// Host + adapter metadata stamp.
    pub hardware: HardwareInfo,
    /// Timing statistics and validity flags.
    pub benchmark: BenchmarkResult,
    /// CPU cross-check result.
    pub verification: VerifyResult,
    /// Stable FNV-1a hash of the exact shader text that ran.
    pub shader_hash: String,
    /// `YYYY-MM-DDTHH:MM:SSZ` capture time.
    pub timestamp_utc: String,
    /// Human-readable descriptions of every adapter that was enumerated.
    pub enumerated_adapters: Vec<String>,
    /// Row id of the stored result (`None` if storage was skipped).
    #[cfg(feature = "sqlite")]
    pub row_id: Option<i64>,
}

/// Run a full probe: set up the GPU, stamp the hardware, benchmark, verify, and
/// (with the `sqlite` feature) store one row.
///
/// The library performs no printing of its own — progress and warnings are
/// surfaced through [`ProbeOutcome::enumerated_adapters`] and
/// [`BenchmarkResult::warnings`] for the caller to render. `cfg.n` is normalized
/// up to a multiple of [`benchmark::TILE_SIZE`] before use.
///
/// # Errors
///
/// Returns [`Error::NoGpu`] when no usable (non-CPU) adapter exists, and (with the
/// `sqlite` feature) [`Error::Db`] if the store fails.
pub fn run_probe(cfg: &Config) -> Result<ProbeOutcome, Error> {
    let mut cfg = cfg.clone();
    cfg.n = util::normalize_n(cfg.n);

    // Hash the exact shipped-and-run shader text (stable FNV-1a, hex).
    let shader_hash = format!("{:016x}", benchmark::fnv1a_64(benchmark::GEMM_WGSL.as_bytes()));

    // GPU setup (also rejects a CPU/software adapter).
    let ctx = benchmark::setup()?;

    // Host + adapter metadata stamp.
    let hardware = hardware::collect(&ctx.adapter_info);

    // Benchmark, then verify a seeded sample against a CPU recomputation.
    let benchmark = benchmark::run(&ctx, &cfg);
    let verification = verify::verify(cfg.n, &benchmark.a, &benchmark.b, &benchmark.c);

    let timestamp_utc = util::utc_now();

    #[cfg(feature = "sqlite")]
    let row_id = {
        let conn = store::open(&cfg.db_path)?;
        Some(store::insert(
            &conn,
            &cfg,
            &hardware,
            &benchmark,
            &verification,
            &shader_hash,
            &timestamp_utc,
        )?)
    };

    Ok(ProbeOutcome {
        enumerated_adapters: ctx.enumerated_adapters.clone(),
        config: cfg,
        hardware,
        benchmark,
        verification,
        shader_hash,
        timestamp_utc,
        #[cfg(feature = "sqlite")]
        row_id,
    })
}
