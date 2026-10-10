#![warn(missing_docs)]
//! Link-only test crate for the synthetic interop fixture.
//!
//! Tests name this crate to propagate the native archive produced by its
//! build script. It exposes generated test inputs and C symbol addresses.
//! The production codegen crate does not depend on it.

#[cfg(not(all(windows, target_env = "msvc")))]
mod class_cases {
    include!(concat!(env!("OUT_DIR"), "/class-symbols.rs"));
    /// Generated fixture directory.
    pub const CLASS_DIRECTORY: &str = env!("OUT_DIR");
    /// ABI sweep mirror.
    pub const CLASS_MIRROR: &str = include_str!(concat!(env!("OUT_DIR"), "/class-mirror.d.ts"));
    /// ABI sweep script.
    pub const CLASS_SCRIPT: &str = include_str!(concat!(env!("OUT_DIR"), "/class-script.ts"));
    /// ABI sweep size.
    pub const CLASS_COUNT: &str = include_str!(concat!(env!("OUT_DIR"), "/class-count.txt"));
    /// Gate boundary mirror.
    pub const GATE_MIRROR: &str = include_str!(concat!(env!("OUT_DIR"), "/gate-mirror.d.ts"));
    /// Gate boundary module.
    pub const GATE_SCRIPT: &str = include_str!(concat!(env!("OUT_DIR"), "/gate-script.ts"));
    /// Gate check count.
    pub const GATE_COUNT: &str = include_str!(concat!(env!("OUT_DIR"), "/gate-count.txt"));
    /// Result sweep mirror.
    pub const RESULT_MIRROR: &str = include_str!(concat!(env!("OUT_DIR"), "/result-mirror.d.ts"));
    /// Result sweep script.
    pub const RESULT_SCRIPT: &str = include_str!(concat!(env!("OUT_DIR"), "/result-script.ts"));
}

#[cfg(not(all(windows, target_env = "msvc")))]
pub use class_cases::*;

/// The directory of the read-root fixture (`specs/blocks/compiler.md` §187
/// rule 3): `read-root.h` and `read-root.c`. Its only user,
/// `cli/tests/read_root.rs`, has the same `cfg`.
#[cfg(not(all(windows, target_env = "msvc")))]
pub const READ_ROOT_DIRECTORY: &str = env!("CARGO_MANIFEST_DIR");
/// The read-root fixture header. The tests bind subsets of it.
#[cfg(not(all(windows, target_env = "msvc")))]
pub const READ_ROOT_HEADER: &str = include_str!("read-root.h");
