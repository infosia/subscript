//! Opt-in native fixture link for the interop golden-capture helper.

use std::path::PathBuf;

#[cfg(unix)]
mod clang_resolver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("CARGO_FEATURE_CAPTURE_INTEROP").is_none() {
        return Ok(());
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        return Ok(());
    }

    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let directory = manifest.join("../corpus/interop");
    let source = directory.join("interop.c");
    let header = directory.join("interop.h");
    let wire_source = directory.join("wire-enum.c");
    let wire_header = directory.join("wire-enum.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", wire_source.display());
    println!("cargo:rerun-if-changed={}", wire_header.display());
    let mut build = cc::Build::new();
    #[cfg(unix)]
    build.compiler(clang_resolver::resolve_capable_clang()?);
    build
        .file(source)
        .file(wire_source)
        .include(directory)
        .std("c11")
        .opt_level(2)
        .compile("subscript_interop_capture_fixture");
    Ok(())
}
