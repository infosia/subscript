# Retained output errors

Contract: compiler.md §268 rules 1–4.

The capture keeps the first write or flush error. It stops file writes after that error.
The memory copy continues. The entry returns an internal error with the file path and the I/O error.
If the script also traps, the same report includes the trap position, rule, and message.
The parent retains ownership of the output file. The abnormal-stop path does not change.

## Evidence

The Red test used a read-only handle to an empty retained file. This handle rejects writes after zero bytes.
The script printed `123` and `456` on separate lines.
Before the fix, the entry returned success and the retained file held zero bytes.
The assertion that requires failure failed. Wall time: 3.17 s. Test time: 0.02 s.

After the fix, the same entry returns an internal error with the file path and the operating-system error.
The control returns success, and both output copies contain `123\n456\n`.
A second entry test checks that the report also names a script exception.
A capture test replaces the failed handle with a writable handle. The second line still reaches memory, but no file write occurs.

## Cost and checks

The production writer stays a `File`. No trait object or writer allocation occurs per line.
Each line adds one error-state check. Each run stores one optional I/O error and clones the retained path once.
The callback test uses two calls and one temporary file. Each entry test compiles one small module.
The control reuses its module. No test fills a file system or starts a child process.
The three new tests took 0.04 s together. Their command took 3.82 s, including the build.
These times measure test cost, not the incremental latency of the error-state check.

| Check | Result | Wall time |
| --- | --- | --- |
| Red: `cargo test -p subscript-codegen --lib retained_output_write_error_fails_run -- --nocapture` | Expected failure; success with empty retained output | 3.17 s |
| `cargo test -p subscript-codegen --lib retained_ -- --nocapture` | 3 passed | 3.82 s |
| `cargo test -p subscript-codegen --lib jit` | 28 passed | 7.56 s |
| `cargo fmt --check` | Passed | 3.97 s |
| `cargo clippy -p subscript-codegen --lib` | Passed; no warnings in changed files | 3.23 s |

Clippy reports 13 warnings in unchanged codegen files and three warnings in compiler files.
No corpus entry or golden changed.

## Follow-up checks

The entry matches the error and retained writer together. It does not call `expect` on the writer.
The test does not print the Red diagnostic.

| Check | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-codegen --lib retained_output_` | 2 passed | 3.81 s |
| `cargo test -p subscript-codegen --lib jit` | 28 passed | 2.97 s |
| `cargo fmt --check` | Passed | 2.10 s |
| `cargo clippy -p subscript-codegen --lib` | Passed; no warnings in changed files | 2.38 s |

Clippy reports 13 warnings in unchanged codegen files, three in compiler files, and ten in runtime files.
