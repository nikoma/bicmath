//! Embed the source revision in published binaries so they identify themselves.
//!
//! Falls back to `unknown` outside a git checkout (for example when building
//! from a source archive). `BICMATH_BUILD_ID` can also be set by the build
//! environment to override.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=BICMATH_BUILD_ID");
    println!("cargo:rerun-if-changed=.git/HEAD");
    let id = std::env::var("BICMATH_BUILD_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(git_revision)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BICMATH_BUILD_ID={id}");
}

fn git_revision() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let revision = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if revision.is_empty() {
        None
    } else {
        Some(revision)
    }
}
