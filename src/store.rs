//! SQLite schema + insert. Creates the DB/table if absent; one row per run.

use rusqlite::Connection;

use crate::benchmark::BenchmarkResult;
use crate::hardware::HardwareInfo;
use crate::verify::VerifyResult;
use crate::Config;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS results (
    id INTEGER PRIMARY KEY,
    timestamp_utc TEXT NOT NULL,
    -- test parameters
    matrix_n INTEGER NOT NULL,
    tile_size INTEGER NOT NULL,
    precision TEXT NOT NULL,
    seed INTEGER NOT NULL,
    warmup_iters INTEGER NOT NULL,
    timed_iters INTEGER NOT NULL,
    timing_method TEXT NOT NULL,
    shader_hash TEXT NOT NULL,
    timestamp_period_ns REAL,
    input_fingerprint TEXT,
    -- results
    median_ms REAL,
    min_ms REAL,
    median_gflops REAL,
    min_gflops REAL,
    raw_timings_json TEXT,
    verification_passed INTEGER,
    max_rel_error REAL,
    run_valid INTEGER,
    -- environment stamp
    gpu_name TEXT,
    gpu_vendor_id INTEGER,
    gpu_device_id INTEGER,
    gpu_driver TEXT,
    gpu_driver_info TEXT,
    gpu_device_type TEXT,
    backend TEXT,
    wgpu_version TEXT,
    os_name TEXT,
    os_version TEXT,
    cpu_brand TEXT,
    cpu_cores INTEGER,
    total_ram_bytes INTEGER,
    raw_adapter_json TEXT
);
"#;

/// Open (creating if needed) the DB at `path` and ensure the schema exists.
pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Insert one fully-stamped result row. Returns the new row id.
pub fn insert(
    conn: &Connection,
    cfg: &Config,
    hw: &HardwareInfo,
    bench: &BenchmarkResult,
    verify: &VerifyResult,
    shader_hash: &str,
    timestamp_utc: &str,
) -> rusqlite::Result<i64> {
    let raw_timings_json =
        serde_json::to_string(&bench.raw_timings_ms).unwrap_or_else(|_| "[]".to_string());

    conn.execute(
        r#"
        INSERT INTO results (
            timestamp_utc,
            matrix_n, tile_size, precision, seed, warmup_iters, timed_iters,
            timing_method, shader_hash, timestamp_period_ns, input_fingerprint,
            median_ms, min_ms, median_gflops, min_gflops, raw_timings_json,
            verification_passed, max_rel_error, run_valid,
            gpu_name, gpu_vendor_id, gpu_device_id, gpu_driver, gpu_driver_info,
            gpu_device_type, backend, wgpu_version, os_name, os_version,
            cpu_brand, cpu_cores, total_ram_bytes, raw_adapter_json
        ) VALUES (
            :timestamp_utc,
            :matrix_n, :tile_size, :precision, :seed, :warmup_iters, :timed_iters,
            :timing_method, :shader_hash, :timestamp_period_ns, :input_fingerprint,
            :median_ms, :min_ms, :median_gflops, :min_gflops, :raw_timings_json,
            :verification_passed, :max_rel_error, :run_valid,
            :gpu_name, :gpu_vendor_id, :gpu_device_id, :gpu_driver, :gpu_driver_info,
            :gpu_device_type, :backend, :wgpu_version, :os_name, :os_version,
            :cpu_brand, :cpu_cores, :total_ram_bytes, :raw_adapter_json
        )
        "#,
        rusqlite::named_params! {
            ":timestamp_utc": timestamp_utc,
            ":matrix_n": cfg.n,
            ":tile_size": crate::benchmark::TILE_SIZE,
            ":precision": "f32",
            ":seed": cfg.seed as i64, // SQLite integers are i64; bit-preserving cast
            ":warmup_iters": cfg.warmup_iters,
            ":timed_iters": cfg.timed_iters,
            ":timing_method": bench.timing_method,
            ":shader_hash": shader_hash,
            ":timestamp_period_ns": bench.timestamp_period_ns,
            ":input_fingerprint": bench.input_fingerprint,
            ":median_ms": bench.median_ms,
            ":min_ms": bench.min_ms,
            ":median_gflops": bench.median_gflops,
            ":min_gflops": bench.min_gflops,
            ":raw_timings_json": raw_timings_json,
            ":verification_passed": verify.passed as i64,
            ":max_rel_error": verify.max_rel_error,
            ":run_valid": bench.run_valid as i64,
            ":gpu_name": hw.gpu_name,
            ":gpu_vendor_id": hw.gpu_vendor_id,
            ":gpu_device_id": hw.gpu_device_id,
            ":gpu_driver": hw.gpu_driver,
            ":gpu_driver_info": hw.gpu_driver_info,
            ":gpu_device_type": hw.gpu_device_type,
            ":backend": hw.backend,
            ":wgpu_version": hw.wgpu_version,
            ":os_name": hw.os_name,
            ":os_version": hw.os_version,
            ":cpu_brand": hw.cpu_brand,
            ":cpu_cores": hw.cpu_cores,
            ":total_ram_bytes": hw.total_ram_bytes as i64,
            ":raw_adapter_json": hw.raw_adapter_json,
        },
    )?;

    Ok(conn.last_insert_rowid())
}
