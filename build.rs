//! Capture the ACTUAL pinned wgpu version at build time.
//!
//! We deliberately do not hand-type the version anywhere in the source. Instead
//! we read the version from our own `Cargo.toml` and emit it as a compile-time
//! env var (`WGPU_VERSION`). Because wgpu is pinned with `=` in Cargo.toml, the
//! requirement string is exact and equals the resolved version, so the binary
//! always records the wgpu it was truly built against.
//!
//! Why Cargo.toml and not Cargo.lock: a published crate ships WITHOUT its
//! Cargo.lock, and when gpu-probe is used as a dependency the build script's
//! directory has no lockfile at all — reading Cargo.lock would silently yield
//! "unknown" and break the comparability invariant. `Cargo.toml` is always
//! present (Cargo includes it in every packaged crate), so this works in both
//! the top-level and the dependency build contexts. This relies on the exact
//! `=` pin; loosening the pin would make the recorded version approximate.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let manifest_path = Path::new(&manifest_dir).join("Cargo.toml");

    // Re-run only when the manifest changes.
    println!("cargo:rerun-if-changed=Cargo.toml");

    let manifest = fs::read_to_string(&manifest_path).unwrap_or_default();

    // Handles both `wgpu = "=30.0.0"` and `wgpu = { version = "=30.0.0", ... }`.
    let version = parse_wgpu_version(&manifest).unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=WGPU_VERSION={version}");
}

/// Pull the pinned wgpu version out of a Cargo.toml body. Returns the version
/// with any leading requirement operator (`=`/`^`/`~`) stripped.
fn parse_wgpu_version(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        // Match the dependency key exactly so we don't catch e.g. `wgpu-core`.
        let rest = match line.strip_prefix("wgpu") {
            Some(r) => r.trim_start(),
            None => continue,
        };
        if !rest.starts_with('=') {
            continue; // not `wgpu = ...`
        }
        // After the `=` assignment, take the version literal: for the table form
        // seek past `version`, for the shorthand form take the first string.
        let after_key = &rest[1..];
        let search = match after_key.find("version") {
            Some(idx) => &after_key[idx..],
            None => after_key,
        };
        let start = search.find('"')? + 1;
        let end = search[start..].find('"')? + start;
        let raw = &search[start..end];
        return Some(raw.trim_start_matches(['=', '^', '~', ' ']).to_string());
    }
    None
}
