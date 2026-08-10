//! Compiles the static archive for native link-input order tests.

use std::path::PathBuf;

#[cfg(unix)]
#[path = "../../clang_resolver.rs"]
mod clang_resolver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join("archive-only.c");
    let header = manifest.join("archive-only.h");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());

    let mut build = cc::Build::new();
    #[cfg(unix)]
    build.compiler(clang_resolver::resolve_capable_clang()?);
    build
        .file(&source)
        .include(&manifest)
        .std("c11")
        .opt_level(2)
        .compile("subscript_archive_probe");

    let archive_name = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        "subscript_archive_probe.lib"
    } else {
        "libsubscript_archive_probe.a"
    };
    let archive_path = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join(archive_name);
    println!(
        "cargo:rustc-env=SUBSCRIPT_ARCHIVE_FIXTURE_PATH={}",
        archive_path.display()
    );
    Ok(())
}
