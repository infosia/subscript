//! Compiles the committed synthetic interop implementation for tests.
//!
//! The `cc` crate selects the C toolchain from the Rust target triple,
//! including MSVC `cl` for `*-pc-windows-msvc`. Keeping this build script
//! in a dev-dependency prevents production codegen consumers from linking
//! the fixture object.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let directory = manifest.join("../../../corpus/interop");
    let source = directory.join("interop.c");
    let header = directory.join("interop.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());

    cc::Build::new()
        .file(&source)
        .include(&directory)
        .std("c11")
        .opt_level(2)
        .compile("subscript_interop_fixture");
}
