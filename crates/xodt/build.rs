//! Embed the Windows application manifest, and do nothing else ever.
//!
//! This is the crate's only build script and `DESIGN.md` §9 is why it is worth
//! reading before adding a second thing to it. Nothing here compiles C: it
//! prints two linker arguments and the linker that was already linking the
//! binary embeds `packaging/windows/odox.manifest`. No resource compiler, no
//! object file, nothing compiled that was not compiled before.
//!
//! **The three applications carry this file byte for byte identically**, and
//! `packaging/preflight.sh` refuses a release where they have drifted. It reads
//! the binary's name from the environment rather than naming one, so the copies
//! have nothing in them to diverge over. A shared build-dependency crate would
//! be the other answer and would cost a crate to hold thirty lines.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

use std::path::Path;

fn main() {
    // The manifest lives with the rest of that platform's files, which is the
    // same rule as staying inside your own directory under `packaging/`.
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/windows/odox.manifest");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", manifest.display());

    // Read from the environment rather than from `cfg!`, because a build script
    // is compiled for the host and `cfg!(windows)` in here answers about the
    // machine doing the building. Cross-checking from Linux with
    // `--target x86_64-pc-windows-msvc` is a thing this repository does, and it
    // would take the wrong branch.
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }

    // The manifest is above this crate's own directory, and `cargo package`
    // copies only what is under a crate root — so a crate published to
    // crates.io and built for Windows would not find it. flyleaf lost a release
    // tag to the same shape with `include_str!`. Rather than fail a build that
    // is otherwise fine, the manifest is skipped when it is not there and the
    // binary is left without it: no DPI declaration, which the certification kit
    // would refuse and a person running `cargo install` will not notice.
    // **A Store build is made from this repository, where the file exists.**
    // `cargo package -p <crate> --list` is the check.
    if !manifest.exists() {
        println!(
            "cargo:warning=no {} — this build declares no DPI awareness",
            manifest.display()
        );
        return;
    }

    // `/MANIFEST:EMBED` is MSVC's, which is why the guard above tests the
    // environment and not just the operating system: a `windows-gnu` target
    // links with something that would not understand it.
    let binary = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    for arg in [
        "/MANIFEST:EMBED".to_owned(),
        format!("/MANIFESTINPUT:{}", manifest.display()),
    ] {
        println!("cargo:rustc-link-arg-bin={binary}={arg}");
    }
}
