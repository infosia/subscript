//! Compiles the committed synthetic-header implementation
//! (`corpus/interop/interop.c`) into a static library named `interop` and
//! links it into every binary that links this crate. The corpus gate's
//! native-library test support takes those addresses and supplies them to
//! the dev JIT; the same native-library value supplies this source to the
//! ship-C compiler, so one implementation serves both tiers
//! (compiler.md §23.5–§23.6).
//!
//! The platform C toolchain is selected by Rust target triple through the
//! `cc` crate (already resolved in `Cargo.lock` and present in the local
//! registry cache — offline-clean, no fetch): the GCC/Clang driver plus
//! `ar` on Unix targets, the MSVC toolchain (`cl`/`lib`) on
//! `*-pc-windows-msvc`. The `-std=c11` dialect pin (compiler.md §11) is
//! carried across via `.std("c11")` (`/std:c11` on MSVC); `CC`/`AR`
//! overrides are honored by the crate natively. A missing toolchain fails
//! the build: `.compile()` panics on toolchain failure, matching the
//! standing gate's "the gate machine is the development machine" discipline
//! (compiler.md §8.3, §11a).

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let interop_dir = manifest.join("../corpus/interop");
    let src = interop_dir.join("interop.c");
    let header = interop_dir.join("interop.h");

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", header.display());

    cc::Build::new()
        .file(&src)
        .include(&interop_dir)
        .std("c11")
        .opt_level(2)
        .compile("interop");
}
