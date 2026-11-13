# Foreign-call scratch release (section 264)

At contract pin `b7ff871c`, the new ReloadSession test reports `UncaughtException` on each of three calls.
The live boundary scratch counts are `[1, 2, 3]`.
The Red assertion fails in 2.49 seconds, including the build.
The test adds the observer API without a change to the pinned foreign-call lowering.

The development tier now skips write-back when a trap is pending, releases scratch, then checks the Call trap.
The trap check and write-back guard read the pending state through one helper.
ReloadSession exposes `live_boundary_scratch_allocations`, with documentation and a direct unit test.
The RunConfig memory accounting changes are removed.
The interpreter does not execute foreign calls; no interpreter change is required.

The JIT test uses a Rust C ABI function with a callback and a nested boundary pointer that needs scratch.
After the fix, all three calls report the same trap and scratch counts `[0, 0, 0]`.
A ship-tier call reports the same trap.
The normal callback control writes back `42` in both tiers.
No corpus entry, golden, or existing boundary scratch test changes.

Checks (wall time includes builds and lock waits):

- `cargo test -p subscript-codegen --test foreign_trap_scratch`: PASS, 3.02 seconds (final run).
- `cargo test -p subscript-codegen --test boundary_scratch_breadth`: PASS, 37.84 seconds.
- `cargo test -p subscript-codegen --lib reload::`: PASS, 34 tests, 3.51 seconds.
- `cargo fmt --check`: PASS, 2.01 seconds.
- `cargo clippy -p subscript-codegen --all-targets`: PASS, 4.28 seconds; no warnings in changed files.

The JIT test runs on all platforms. The ship controls run on Unix.
Only the ship controls use a test-local C source; the ship compiler selects the C compiler through its existing configuration.
The test does not start a C compiler directly or load a shared library.
A Drop guard removes the temporary ship fixture directory, including after an assertion failure.
The first run failed symbol validation because the ship library had no symbol entry (4.14 seconds).
The final run supplies the Rust function address for symbol validation; ship execution compiles the C source.

The StringView write-back defers its pending allocation trap until after scratch release.
The other write-back helpers emit no trap check or runtime exit before release.
The pending-state guard exists only for calls with write-backs or targets, as in the C tier.

All check processes exited. No background task remains.
