<!-- §280 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 280. The example host tests use the CLI that cargo built for the test

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P1, "remove nested Cargo builds from example tests").
Iteration speed is a goal, and a gate's cost is part of its design
(core principle 15).

Problem, read in the code and measured. `examples/tests/gate.rs` runs
`examples/host/build.sh` and `examples/context-per-scene/build.sh`.
Each script runs `cargo build --release -p subscript-cli` and then the
release CLI. The debug test step therefore builds the release CLI. In
the full gate record of 2026-11-13 at `8a36c8c1`, the example suite
took 59.96 s in debug and 4.47 s in release. After a change to the
compiler or the code generator, the quick gate (debug only) builds the
whole release CLI inside this test. The full gate builds it in the
release step in any case, so there the cost moves and the cargo lock
is held twice.

### 280.1 Rules

1. Each of the two scripts reads `SUBSCRIPT` from the environment. If
   it is set, the script runs that CLI and runs no `cargo` command. If
   it is not set, the script builds and runs the release CLI as before.
   A user who runs the script by hand sees no change.
2. The two host tests move to the `subscript-cli` package
   (`cli/tests/example_hosts.rs`). Each test runs its script with
   `SUBSCRIPT` set to `CARGO_BIN_EXE_subscript`, the CLI that cargo
   built for this test profile.
3. Each test keeps its observation: the script exits 0, and its stdout
   equals the committed `expected.txt` of the example.
4. The CLI still finds the runtime archive by its own rules
   (`cli/src/runtime_paths.rs`). This section does not change them.
5. `examples/README.md` states the `SUBSCRIPT` override in one
   sentence.

### 280.2 Acceptance

1. A text check reads the two scripts. Each has exactly one `cargo`
   command, inside the branch where `SUBSCRIPT` is not set.
2. A control: a test runs one script with `SUBSCRIPT` set to a stub
   that prints a fixed line and exits 0. The script output holds that
   line, and the stub received the arguments `build --source`.
3. `cargo test -p subscript-cli --test example_hosts` passes in debug
   and in release. The wall time of the debug run is recorded with
   the release CLI out of date (touch a compiler source file first)
   before and after the change, in
   `specs/tracking/test-cost-review-2026-11-14.md`.
4. The full gate passes. No golden moves.

### 280.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. It found that both
profiles link the same release runtime archive, that the two host runs
write to separate directories, and that the text check and the stub
control can fail. Five MINOR findings:

1. Realistic, open: a golden mismatch prints bytes, not text, and no
   longer names the `expected.txt` path.
2. Contrived, open: if an assertion of the stub control fails, its
   scratch directory stays.
3. Open: no Windows run of the new tests is recorded.
4. Realistic, fixed: the tracking note had a second verb for the check
   concept, a fragment, and no result line.
5. No change: the tracking table has no after row with a touched
   compiler for the examples package. Acceptance item 3 does not ask
   for one.

### 280.4 Open

1. The host test failure prints the stdout as bytes and does not name
   the golden path (§280.3 item 1).
2. The stub control leaves its scratch directory if an assertion fails
   (§280.3 item 2).
3. The new tests have no recorded Windows run (§280.3 item 3).
