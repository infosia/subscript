# Runtime target directory (§262)

The CLI build command passes `--target-dir <root>/target`.
The archive lookup uses the same directory function.
The five output-reader scripts pass explicit target directories to eleven cargo builds.
No corpus entry or golden changes.

## Red

At contract pin `31467f2e`, command extraction kept the original arguments.
The new library unit test failed: `runtime build must pass --target-dir`.
Result: 0 passed, 1 failed; test time 0.00 s; command time 1.72 s.

## Checks

| Command | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-cli --lib runtime_build_uses_lookup_target_directory` | 1 passed | 0.33 s |
| `cargo test -p subscript-cli --test script_target_dir -- --nocapture` | 2 passed | 0.79 s |
| `cargo test -p subscript-cli` | Passed | 69.97 s |
| `cargo fmt --check` | Passed | 3.53 s |
| `cargo clippy -p subscript-cli --all-targets` | Passed; no warnings in changed files | 4.52 s |
| `sh -n examples/host/build.sh` | Passed | 0.013584 s |
| `sh -n examples/context-per-scene/build.sh` | Passed | 0.005847 s |
| `sh -n examples/hot-reload/run.sh` | Passed | 0.005383 s |
| `sh -n codegen/device-link.sh` | Passed | 0.011003 s |

The text test runs `git ls-files` once and reads each tracked shell file.
Cost is one child process plus linear source traversal.
The scan read eight files, 28,747 bytes, and twelve build commands in 20.190541 ms.
Controls cover plain commands, `cd`, `if`, `exec`, `CARGO`, line continuation, and comments.
The allowlist permits `tools/gate.sh` because the script does not read the build output.
The later `cargo test` reuses the same target directory.
A control checks that an allowlist row fails when no command matches it.

The full CLI command ran existing `cli/tests/gate.rs` tests that invoke `tools/gate.sh` with tool stubs.
The example scripts and `device-link.sh` did not run.
All commands started for this task completed.

## Full CLI test results

| Suite | Result and test time |
| --- | --- |
| Running unittests src/lib.rs | test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s |
| Running unittests src/main.rs | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| Running tests/bind_callback_lifetime.rs | test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.66s |
| Running tests/bind_completion.rs | test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s |
| Running tests/boundary.rs | test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s |
| Running tests/commands.rs | test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s |
| Running tests/downstream_script.rs | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s |
| Running tests/file_module.rs | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s |
| Running tests/gate.rs | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 46.16s |
| Running tests/read_records.rs | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s |
| Running tests/main_thread/read_root.rs | test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.45s |
| Running tests/script_target_dir.rs | test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| Running tests/watch.rs | test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s |
| Doc-tests subscript_cli | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

## Total scan checks

| Command | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-cli --test script_target_dir -- --nocapture` | 1 passed; 1 failed; test time 0.02 s; missing flag in `tools/gate.sh:289` | 1.298303 s |
| `sh -n spike/mobile-link/link.sh` | Passed | 0.006063 s |
| `cargo fmt --check` | Passed | 2.121490 s |


## Allowlist checks

The allowlist contains file names and reasons for builds without `--target-dir`.
A row matches only a build without that option in the named file.
The test rejects rows with no match or no reason.
The stale-row control covers an absent file, no build, an explicit target directory, and a different file.

| Command | Result | Wall time |
| --- | --- | --- |
| `cargo test -p subscript-cli --test script_target_dir -- --nocapture` | 3 passed; 0 failed; test time 0.02 s | 1.444292 s |
| `cargo fmt --check` | Passed | 2.213496 s |
| `cargo clippy -p subscript-cli --all-targets` | Passed; no warnings in changed files | 0.426765 s |

The initial format check failed on test layout in 2.484246 s. Rustfmt corrected the layout before the checks above.
The scan read eight files, 28,747 bytes, and twelve build commands in 18.307625 ms.
The scan found no build commands in the other two scripts.
The list below shows the command tokens after quote removal.

```text
codegen/device-link.sh: build #1: cargo build --target-dir $TARGET_DIR --offline --release -p subscript-cli
codegen/device-link.sh: build #2: cargo build --target-dir $TARGET_DIR --offline --release -p subscript-runtime --target aarch64-apple-ios
codegen/device-link.sh: build #3: cargo build --target-dir $TARGET_DIR --offline --release -p subscript-runtime
codegen/device-link.sh: build #4: cargo build --target-dir $TARGET_DIR --offline --release -p subscript-runtime
codegen/device-link.sh: build #5: cargo build --target-dir $TARGET_DIR --offline --release -p subscript-runtime --target aarch64-linux-android
examples/context-per-scene/build.sh: build #1: cargo build --target-dir $REPO_ROOT/target --offline --release -p subscript-cli 1>
examples/host/build.sh: build #1: cargo build --target-dir $REPO_ROOT/target --offline --release -p subscript-cli 1>
examples/hot-reload/run.sh: build #1: cargo build --target-dir $REPO_ROOT/target --offline -p subscript-cli 1>
spike/mobile-link/link.sh: build #1: cargo build --target-dir $TARGET_DIR --offline --release --manifest-path $SPIKE_DIR/Cargo.toml
spike/mobile-link/link.sh: build #2: cargo build --target-dir $TARGET_DIR --offline --release --target aarch64-linux-android
spike/mobile-link/link.sh: build #3: cargo build --target-dir $TARGET_DIR --offline --release --target aarch64-apple-ios
tools/gate.sh: build #1: run build $CARGO build --offline --locked --workspace --all-targets
```

`tools/gate.sh` did not change. No example or device script ran.
All commands started for this task completed.
