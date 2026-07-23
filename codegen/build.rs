//! Compiles the committed synthetic-header implementation
//! (`corpus/interop/interop.c`) into a static library and links it into
//! every binary that links this crate. That is what makes the foreign
//! symbols (`subDevice*`) resolvable *by address* inside the dev-JIT test
//! process: `jit.rs` takes their addresses and registers them with the
//! JIT. The ship-C tier links the same source separately (`run_c_aot`),
//! so one C implementation serves both tiers (compiler.md §12.4).
//!
//! The platform C compiler/archiver are invoked directly (no build
//! dependency, offline-clean); their absence fails the build, matching
//! the standing gate's "the gate machine is the development machine"
//! discipline (compiler.md §8.3).

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let interop_dir = manifest.join("../corpus/interop");
    let src = interop_dir.join("interop.c");
    let header = interop_dir.join("interop.h");
    let obj = out.join("interop.o");
    let lib = out.join("libinterop.a");

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");

    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let ar = std::env::var_os("AR").unwrap_or_else(|| "ar".into());

    let compile = Command::new(&cc)
        .arg("-O2")
        .arg("-fPIC")
        .arg("-I")
        .arg(&interop_dir)
        .arg("-c")
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("running the C compiler ({cc:?}) failed: {e}"));
    assert!(compile.success(), "compiling {} failed", src.display());

    // Rebuild the archive from scratch so a stale member is never reused.
    let _ = std::fs::remove_file(&lib);
    let archive = Command::new(&ar)
        .arg("crs")
        .arg(&lib)
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("running the archiver ({ar:?}) failed: {e}"));
    assert!(archive.success(), "archiving {} failed", lib.display());

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=interop");
}
