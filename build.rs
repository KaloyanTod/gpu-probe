//! Capture the ACTUAL resolved wgpu version at build time.
//!
//! We deliberately do not hand-type the version anywhere in the source. Instead
//! we read the version cargo actually resolved from Cargo.lock and emit it as a
//! compile-time env var (`WGPU_VERSION`). Because wgpu is pinned with `=` in
//! Cargo.toml, this string is exact and it changes if (and only if) the pin
//! changes. The binary therefore always records the wgpu it was truly built
//! against.

use std::fs;

fn main() {
    // Re-run only when the lockfile changes.
    println!("cargo:rerun-if-changed=Cargo.lock");

    let lock = fs::read_to_string("Cargo.lock").unwrap_or_default();

    // Cargo.lock is a series of `[[package]]` blocks; within a block `name`
    // precedes `version`. Find the wgpu block and take its version line.
    let mut version = String::from("unknown");
    let mut in_wgpu_block = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_wgpu_block = false;
        } else if line == "name = \"wgpu\"" {
            in_wgpu_block = true;
        } else if in_wgpu_block && line.starts_with("version = ") {
            version = line
                .trim_start_matches("version = ")
                .trim_matches('"')
                .to_string();
            break;
        }
    }

    println!("cargo:rustc-env=WGPU_VERSION={version}");
}
