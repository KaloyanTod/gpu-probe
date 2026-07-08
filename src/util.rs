//! Small pure helpers shared by the library and the CLI. Kept dependency-free
//! (no chrono) and deterministic so they are trivially unit-testable without a GPU.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::benchmark::TILE_SIZE;

/// Normalize `n` to a positive multiple of the tile size. Zero maps to one tile;
/// a non-multiple rounds UP to the next multiple (the tiled shader assumes
/// `n % TILE_SIZE == 0`). Idempotent: an already-valid `n` is returned unchanged.
pub fn normalize_n(n: u32) -> u32 {
    if n == 0 {
        return TILE_SIZE;
    }
    if n % TILE_SIZE == 0 {
        n
    } else {
        ((n / TILE_SIZE) + 1) * TILE_SIZE
    }
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`, without pulling in chrono. Uses
/// Howard Hinnant's civil-from-days algorithm.
pub fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    format_utc(secs)
}

/// Format a Unix timestamp (seconds) as `YYYY-MM-DDTHH:MM:SSZ`. Split out from
/// [`utc_now`] so it can be tested against known epochs.
pub fn format_utc(secs: i64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rounds_up_to_tile_multiple() {
        assert_eq!(normalize_n(0), TILE_SIZE);
        assert_eq!(normalize_n(1), TILE_SIZE);
        assert_eq!(normalize_n(TILE_SIZE), TILE_SIZE);
        assert_eq!(normalize_n(TILE_SIZE + 1), TILE_SIZE * 2);
        assert_eq!(normalize_n(1024), 1024);
        // Idempotent.
        assert_eq!(normalize_n(normalize_n(1000)), normalize_n(1000));
    }

    #[test]
    fn format_utc_known_epochs() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        // 2021-01-01T00:00:00Z
        assert_eq!(format_utc(1_609_459_200), "2021-01-01T00:00:00Z");
    }
}
