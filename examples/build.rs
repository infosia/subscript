//! Compiles the neutral engine facade into the examples gate process.
//!
//! The `cc` crate selects the C compiler from the Rust target triple,
//! including MSVC `cl` for `*-pc-windows-msvc`. Its C11 setting exercises
//! the facade's target-specific thread-local spelling on every gate host.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let engine = manifest.join("engine");
    let source = engine.join("engine.c");
    let header = engine.join("engine.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());

    cc::Build::new()
        .file(&source)
        .include(&engine)
        .std("c11")
        .opt_level(2)
        .compile("subscript_examples_engine");
}
