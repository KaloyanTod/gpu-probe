# gpu-probe

A small, reproducible GPU **matrix-multiply benchmark** and measurement
instrument. It runs a tiled f32 GEMM (`C = A * B`) on a real GPU via
[`wgpu`](https://wgpu.rs), collects a full hardware/software stamp, verifies the
result on the CPU, and records **one fully-stamped row per run** into a local
SQLite database.

This is an instrument, not a leaderboard: correctness and completeness of the
recorded metadata matter more than raw speed. Every knob that can affect a
number (matrix size, seed, warm-up/timed counts, timing method, shader hash,
wgpu version, backend, driver, …) is stored alongside the number so runs can be
judged comparable — or not — after the fact.

## Build & run

```sh
cargo build --release
cargo run --release            # default: N=1024, 3 warm-up, 20 timed, ./gpu-probe.db
```

### First-run requirement: a real GPU stack

`wgpu` drives the machine's **native graphics stack**. The first run needs
working system drivers for at least one of Vulkan / Metal / DX12 / OpenGL to be
present, or no adapter will be found. A CPU/software fallback adapter (e.g.
"Microsoft Basic Render Driver") is **deliberately rejected** — a CPU pretending
to be a GPU must never produce a score. If only such an adapter exists,
`gpu-probe` prints a `no_gpu` message and exits non-zero.

### Options

```
--n <N>          Square matrix size, rounded UP to a multiple of 16. Default 1024.
--seed <U64>     Deterministic input seed (decimal or 0x-hex). Default 0x0123456789ABCDEF.
--warmup <N>     Untimed warm-up iterations. Default 3.
--iters <N>      Timed iterations. Default 20.
--db <PATH>      SQLite database path. Default ./gpu-probe.db.
-h, --help       Print help.
```

Example:

```sh
cargo run --release -- --n 2048 --iters 50 --seed 0xDEADBEEF --db results.db
```

Exit code is non-zero if no usable GPU is found, if verification fails, or if the
run is flagged invalid (see below).

## What gets measured

- **Deterministic inputs.** A and B are generated from the u64 seed with an
  inline **SplitMix64** PRNG (integer-only) mapped to f32 in `[-1, 1)`. The same
  seed produces **bit-identical** matrices on every machine, OS, and CPU — this
  is what makes cross-machine scores comparable. A fingerprint (hash of the
  first 256 bytes of A) is stored so two machines can *prove* they used identical
  inputs.
- **Warm-up then timed.** Warm-up dispatches run untimed (and are forced to
  complete) to reach steady clocks/caches; they are never pooled into results.
  Then the timed iterations run, and the tool reports the **median** and the
  **minimum** (fastest) time. The fast tail is the cleanest hardware signal —
  interference only slows runs down. All raw per-iteration timings are stored.
- **GFLOP/s** is computed from the chosen time using `flops = 2 * N^3`.

### Timing method — READ THIS BEFORE COMPARING NUMBERS

Two timing methods exist; which one was used is recorded in `timing_method`:

- `timestamp_query` — the GPU's `TIMESTAMP_QUERY` feature is available, so the
  compute pass is timed **on-device** (begin/end timestamps around the pass).
  `timestamp_period_ns` records the tick→ms conversion factor for auditing.
- `wall_clock` — the feature is absent, so timing falls back to a CPU
  `Instant` around `submit + poll(Wait)`. This includes a little submit/sync
  overhead.

> ⚠️ **`wall_clock` and `timestamp_query` numbers are NOT interchangeable and
> must never be pooled in analysis.** Always filter on `timing_method`.

### Validity guards

A run is flagged `run_valid = 0` (and a warning printed) if any of:
- any iteration measured `elapsed_ms <= 0` (clock skew / tick wrap) and more
  than half the iterations were discarded;
- no valid timed iterations remained;
- `min_gflops` exceeds a crude universal sanity ceiling (200000 GFLOP/s) — well
  above any current single consumer GPU for f32, so exceeding it means the
  timing is broken, not that the GPU is that fast.

Verification recomputes a small **seeded sample of 32 output cells** on the CPU
and compares within a 1e-3 relative tolerance (GPU float reduction order differs,
so exact equality is the wrong test). A failing run is **still stored**, marked
`verification_passed = 0`.

## The comparability invariant

> A score is only comparable to another score with the same **`shader_hash`**,
> **`wgpu_version`**, **`backend`**, and **`timing_method`**.

## Inspecting results

The DB is plain SQLite. Example query dumping the key columns for one machine:

```sql
SELECT
    id, timestamp_utc, gpu_name, backend, timing_method,
    matrix_n, median_ms, min_ms, median_gflops, min_gflops,
    verification_passed, run_valid, shader_hash, wgpu_version, input_fingerprint
FROM results
WHERE gpu_name = 'Quadro M1000M'
  AND timing_method = 'timestamp_query'
ORDER BY min_gflops DESC;
```

```sh
sqlite3 gpu-probe.db "SELECT id, gpu_name, backend, timing_method, min_gflops, run_valid FROM results;"
```

Every column, plus the full raw `wgpu` adapter dump (`raw_adapter_json`) and all
per-iteration timings (`raw_timings_json`), is preserved so no field is ever
lost even if it wasn't given its own column.

## Project layout

```
src/
  main.rs       orchestration: collect → benchmark → verify → store → print
  hardware.rs   host + adapter metadata
  benchmark.rs  wgpu setup, deterministic inputs, tiled GEMM dispatch, timing
  verify.rs     CPU reference check on a seeded sample
  store.rs      SQLite schema + insert
  shaders/
    gemm.wgsl   tiled 16x16 f32 GEMM compute shader
build.rs        captures the resolved (pinned) wgpu version at build time
```

## Notes on reproducibility

- `wgpu` is pinned **exactly** (`=`) in `Cargo.toml` so its version cannot drift
  silently; `build.rs` reads the actually-resolved version from `Cargo.lock` and
  the binary records it in every row.
- The shader is compiled in via `include_str!` and hashed with stable FNV-1a, so
  the shader that *ran* is exactly the shader that's *hashed*. Change one
  character and the hash changes, correctly invalidating comparability.




This README was generated by Claude.
