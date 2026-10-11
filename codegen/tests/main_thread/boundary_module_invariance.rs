//! The test main of `tests/boundary_module_invariance.rs` (compiler.md §190.1 rule 3).
//!
//! That file excludes itself on windows-msvc with an inner `cfg`, and a
//! `harness = false` binary needs a `main` on every target. This file
//! gives it: the tests of `tests/boundary_module_invariance.rs` run in the two phases of the
//! harness. On windows-msvc it is an empty `main` with no test.

// compiler.md §190.1 rule 3: a test function that the phase list does
// not name fails the build.
#![deny(dead_code)]

#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "../support/main_thread.rs"]
mod main_thread;

#[cfg(not(all(windows, target_env = "msvc")))]
#[path = "../boundary_module_invariance.rs"]
mod body;

#[cfg(not(all(windows, target_env = "msvc")))]
fn main() -> std::process::ExitCode {
    body::main()
}

#[cfg(all(windows, target_env = "msvc"))]
fn main() {}
