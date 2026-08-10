#![warn(missing_docs)]
//! Test-only static archive for native link-input order tests.

/// The path to the static archive that the build script creates.
pub const ARCHIVE_PATH: &str = env!("SUBSCRIPT_ARCHIVE_FIXTURE_PATH");

/// The directory that contains the fixture header.
pub const CRATE_DIRECTORY: &str = env!("CARGO_MANIFEST_DIR");
