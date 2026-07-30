//! Opt-in native fixture link for the interop golden-capture helper.

use std::path::PathBuf;

fn main() {
    if std::env::var_os("CARGO_FEATURE_CAPTURE_INTEROP").is_none() {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        return;
    }

    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let directory = manifest.join("../corpus/interop");
    let source = directory.join("interop.c");
    let header = directory.join("interop.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());
    cc::Build::new()
        .file(source)
        .include(directory)
        .std("c11")
        .opt_level(2)
        .compile("subscript_interop_capture_fixture");
}
