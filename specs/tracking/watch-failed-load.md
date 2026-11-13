# Failed-load watch paths

Contract: compiler.md §266 rules 1–7.

The loader returns each attempted disk path and its stamp on success and failure.
The watch loop replaces its watch set with those paths and the entry after each failed load.
The loader runs no check. The watch session reports an absent import.
After an accepted check, the watch set contains the loaded files.
After a rejected check, it contains the attempted paths and the entry.
An entry read failure at startup exits 2.

## Red

At `6f5106ca`, the parser-error test ran one initial load.
A module-only fix ran no load: the load count stayed at 1 instead of 2.
The first test command failed in 1.74 seconds.

The controlled-loop tests also ran against the pin's loader and watch-set behavior.
Only the load and poll injection scaffold differed from the pin.
Both tests failed: a module-only fix and an absent-module creation produced no program output.
The command took 1.31 seconds; the tests took 0.02 seconds.

## Green and cost

The tests control each poll and count each load. They use no real sleep.
Each first-load case uses two loads: the initial attempt and the module-only fix.
Each later-load case uses three loads, including the entry edit that adds the import.
An unrelated edit adds no load.
The first-load cases print `fixed` after the module fix or creation.
The later-load cases reach the existing module-set refusal after the module fix or creation.

| Check | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-cli --lib failed_load_` | 3 passed; test time 0.02 s | 0.17 s |
| `cargo test -p subscript-cli --test watch` | 15 passed; test time 1.64 s | 2.91 s |
| `cargo test -p subscript-cli --lib` | 15 passed; test time 0.05 s | 1.21 s |
| `cargo fmt --check` | Pass | 2.38 s |
| `cargo clippy -p subscript-cli --all-targets` | Pass; no warnings in changed files | 1.17 s |

Clippy reports existing warnings in other crates.
No corpus entry or golden file changed.

## Additional checks

A poll with no file change adds no load and prints no second failure.
The controlled tests assert one failure message per change. They add no real sleep.
The absent-import case loads successfully, then the watch session rejects it.
The enabled `node:fs/promises` control compares the complete `run` diagnostic with the configured checker diagnostic.
It also passes with the loader from `914c753a`, through an adapter that returns the new result type.
The missing-entry test calls `run --watch` and checks exit 2 without a wait message.

| Check | Result | Wall time |
| --- | --- | --- |
| Pin loader: enabled-module diagnostic control | 1 passed | 1.20 s |
| `cargo test -p subscript-cli --lib failed_load_` | 5 passed; test time 0.05 s | 1.37 s |
| `cargo test -p subscript-cli --test watch` | 15 passed; test time 1.68 s | 3.78 s |
| `cargo test -p subscript-cli --lib` | 17 passed; test time 0.05 s | 1.27 s |
| `cargo fmt --check` | Pass | 2.44 s |
| `cargo clippy -p subscript-cli --all-targets` | Pass; no warnings in changed files | 2.10 s |

The four final checks ran concurrently. Wall times include Cargo lock waits.
Clippy reports existing warnings in other crates.
