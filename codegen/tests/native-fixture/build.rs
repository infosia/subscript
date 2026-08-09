//! Compiles the committed synthetic interop implementation for tests.
//!
//! The `cc` crate selects the C toolchain from the Rust target triple,
//! including MSVC `cl` for `*-pc-windows-msvc`. Keeping this build script
//! in a dev-dependency prevents production codegen consumers from linking
//! the fixture object.

use std::path::PathBuf;

#[cfg(unix)]
#[path = "../../clang_resolver.rs"]
mod clang_resolver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let directory = manifest.join("../../../corpus/interop");
    let source = directory.join("interop.c");
    let header = directory.join("interop.h");
    let external_source = directory.join("external-device.c");
    let external_header = directory.join("external-device.h");
    let wire_source = directory.join("wire-enum.c");
    let wire_header = directory.join("wire-enum.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", external_source.display());
    println!("cargo:rerun-if-changed={}", external_header.display());
    println!("cargo:rerun-if-changed={}", wire_source.display());
    println!("cargo:rerun-if-changed={}", wire_header.display());

    // interop.c uses _Float16, which MSVC cl cannot compile; the fixture is
    // never linked on this target (its dependency edges are gated off
    // windows-msvc), so skip building it. compiler.md §11c.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        return Ok(());
    }

    let mut build = cc::Build::new();
    #[cfg(unix)]
    build.compiler(clang_resolver::resolve_capable_clang()?);
    build
        .file(&source)
        .file(&external_source)
        .file(&wire_source)
        .include(&directory)
        .std("c11")
        .opt_level(2)
        .compile("subscript_interop_fixture");
    Ok(())
}
