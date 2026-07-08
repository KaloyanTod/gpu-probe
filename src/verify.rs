//! CPU reference check on a small, seeded sample of output cells.
//!
//! We do NOT recompute the whole matrix (that would defeat the point of GPU
//! timing and is slow). We sample a fixed number of (row, col) cells, compute
//! each C[row][col] directly from A and B on the CPU, and compare within a
//! relative tolerance — GPU float reduction order differs from ours, so exact
//! equality is the wrong test.

/// Relative tolerance. GPU accumulates in a different order, so small relative
/// error is expected and fine; large error means the kernel is wrong.
pub const REL_TOLERANCE: f32 = 1.0e-3;

/// Number of sampled cells.
pub const SAMPLE_COUNT: usize = 32;

pub struct VerifyResult {
    pub passed: bool,
    pub max_rel_error: f64,
}

/// SplitMix64 again, used here only to pick sample coordinates deterministically
/// (a separate, fixed seed so the sample set is stable across runs).
#[inline]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Verify a seeded sample of C cells against a CPU recomputation.
pub fn verify(n: u32, a: &[f32], b: &[f32], c: &[f32]) -> VerifyResult {
    let n = n as usize;
    // Fixed seed => same sampled cells every run, independent of the input seed.
    let mut state: u64 = 0xD1B54A32D192ED03;

    let mut max_rel_error: f64 = 0.0;

    for _ in 0..SAMPLE_COUNT {
        let row = (splitmix64(&mut state) as usize) % n;
        let col = (splitmix64(&mut state) as usize) % n;

        // Reference: C[row][col] = sum_k A[row][k] * B[k][col].
        let mut acc: f32 = 0.0;
        for k in 0..n {
            acc += a[row * n + k] * b[k * n + col];
        }

        let gpu = c[row * n + col];
        // Relative error, guarded against a zero reference.
        let denom = acc.abs().max(1.0e-6);
        let rel = ((gpu - acc).abs() / denom) as f64;
        if rel > max_rel_error {
            max_rel_error = rel;
        }
    }

    VerifyResult {
        passed: max_rel_error <= REL_TOLERANCE as f64,
        max_rel_error,
    }
}
