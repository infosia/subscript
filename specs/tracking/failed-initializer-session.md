# Failed initializer session

Contract: compiler.md §272 rules 1 to 5.

## Evidence

The contract pin is `fe68e2a8`. The Red test used its production code.
The test threw before `const g: i32 = 7`, then called an export that printed `g`.
The call succeeded and printed `0\n`. The refusal assertion failed. Wall time: 2.33 s.
No corpus entry or golden changed.

## Change

The session retains the initializer trap. Each entry refuses with `RunError::Trap` before script code runs.
The error message names the initializer trap. Its stdout contains the current undrained output.
A reload after failed initialization builds a fresh session and runs the initializer from the start.
The replacement retains the module configuration, native libraries, and file provider.
A rejected build leaves the old session intact. A repeated initializer trap blocks entries again.
A completed session retains the existing reload path and global values.
Watch uses its existing trap path for the refused call and waits for an edit.
The unit tests moved into `codegen/src/reload/tests.rs` to keep each source file below 2,000 lines.

## Cost and results

Times are command wall times. Test-body times exclude build and command startup costs.
Each focused test uses JIT compilation because the checks require script execution.

| Check | Result | Wall time | Test-body time | Cost |
| --- | --- | --- | --- | --- |
| Red: acceptance 1 | Expected failure; output `0\n` | 2.33 s | 0.04 s | One JIT build and one entry call |
| `failed_initializer_blocks_entries`: acceptance 2 | Pass | 0.11 s | 0.01 s | One JIT build and one refused entry |
| `failed_initializer_reload_starts_again`: acceptance 3 | Pass | 0.12 s | 0.03 s | Three JIT builds, one checker rejection, all entry guards |
| `completed_initializer_reload_preserves_globals`: acceptance 4 | Pass | 0.11 s | 0.02 s | Two JIT builds and two entry calls |
| `watch_recovers_from_failed_initializer`: acceptance 5 | Pass | 2.04 s | 0.06 s | Three watch generations |
| `cargo test -p subscript-codegen --lib reload` | 37 passed | 2.49 s | 0.13 s | Reload regression suite |
| `cargo test -p subscript-cli --test watch` | 16 passed | 5.52 s | 1.64 s | Watch transitions and child-process checks |
| `cargo fmt --check` | Pass | 2.01 s | — | Workspace format check |
| `cargo clippy -p subscript-codegen -p subscript-cli --lib --tests` | Pass; no warnings in changed files | 3.71 s | — | Library and test static checks |

Clippy reports warnings in unchanged files. No full CLI suite or `tools/gate.sh` ran.

## Additional checks

A failed initializer now takes the rebuild path before the live-worker check (§272 rule 3).
Context drop closes and joins the old workers before it releases their JIT modules.
The session retains unread stdout bytes across each rebuild, before the new initializer output.
Trap reports include those bytes. A take removes those bytes once.
The allocation test compares the rebuilt session with a fresh session of the fixed program.

The checker rejects Worker globals with S100. The worker test spawns a worker inside a global initializer function, then throws.
The worker remains live before reload. The fixed reload succeeds.
The stdout test uses four JIT builds and checks repeated retries, byte order, and no duplicate take.
The allocation test now uses four JIT builds, including the fresh-session comparison.
The worker test uses two JIT builds and one worker thread.

| Check | Result | Wall time | Suite time |
| --- | --- | --- | --- |
| Initial reload test run | 38 passed; Worker global test rejected with S100 | 4.61 s | 0.14 s |
| `cargo test -p subscript-codegen --lib reload` | 39 passed | 2.89 s | 0.55 s |
| `cargo test -p subscript-cli --test watch` | 16 passed | 5.32 s | 1.64 s |
| `cargo fmt --check` | Pass | 2.01 s | — |
| `cargo clippy -p subscript-codegen -p subscript-cli --lib --tests` | Pass; no warnings in changed files | 4.64 s | — |

Clippy reports warnings in unchanged files. No corpus entry or golden changed.
