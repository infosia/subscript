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

mod cases;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let directory = manifest.join("../../../corpus/interop");
    let source = directory.join("interop.c");
    let header = directory.join("interop.h");
    let external_source = directory.join("external-device.c");
    let external_header = directory.join("external-device.h");
    let buffer_source = directory.join("host-buffer-completion.c");
    println!("cargo:rerun-if-changed={}", buffer_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        directory.join("host-buffer-completion.h").display()
    );
    let completion_source = directory.join("host-completion.c");
    println!("cargo:rerun-if-changed={}", completion_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        directory.join("host-completion.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        directory.join("abi-pressure.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        directory.join("abi-pressure.h").display()
    );
    let boundary_source = directory.join("boundary-values.c");
    println!("cargo:rerun-if-changed={}", boundary_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        directory.join("boundary-values.h").display()
    );
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

    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    println!("cargo:rerun-if-changed=cases.rs");
    cases::generate(&out)?;
    let mut build = cc::Build::new();
    #[cfg(unix)]
    build.compiler(clang_resolver::resolve_capable_clang()?);
    build
        .define("SUBSCRIPT_INTEROP_LIBRARY_ONLY", None)
        .file(out.join("class-sweep.c"))
        .file(out.join("class-results.c"))
        .include(&out)
        .file(&source)
        .file(&completion_source)
        .file(&buffer_source)
        .file(&boundary_source)
        .file(directory.join("abi-pressure.c"))
        .file(&external_source)
        .file(&wire_source)
        .include(&directory)
        .std("c11")
        .opt_level(2)
        .compile("subscript_interop_fixture");
    Ok(())
}
