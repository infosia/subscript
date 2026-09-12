<!-- §85 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 85. One gate command, two shapes

*(Owner decision 2026-09-05.)* Origin: the development-cost review of
2026-09-05 (`specs/tracking/development-cost-review-2026-09-05.md`,
finding 3).

Measured at `3677d1f`, this host. The three commands a landing note
calls "the gate" run three different sets:

| Command | Interpreter corpus | Performance thresholds |
|---|---|---|
| `cargo test` (debug) | 124 runnable entries | skipped, `perf_gate.rs` line 3 |
| `cargo test --release` | skipped, `lir.rs` line 2020 | run |
| `--release` with `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1` | 125 entries | run |

A skip prints a free-text line, and nothing counts the lines. The
q35 record (`specs/tracking/q35-string-messages.md`) cites a release
pass at a pin where one release test fails 3 of 3 runs. The record
does not say which command ran. §8.3 states the gate as a rule and
names no command. The 2026-08-26 decision (s68 tracking, "Owner
decisions") separates the round's targeted run from the one full run
and names no command either.

### 85.1 Rule

1. **Two shapes, one script.** `tools/gate.sh quick` and
   `tools/gate.sh full` are the gate. A landing note, a tracking
   note, or a report cites a gate result only as the verdict line of
   a record that this script wrote. A test count quoted from any
   other command is not a gate result.
2. **`quick` is the round gate.** It runs, in this order, and stops
   at the first non-zero exit:
   `cargo fmt --check`;
   `cargo build --offline --locked --workspace --all-targets`, and
   one rustc warning is a failure;
   `cargo test --offline --locked --workspace --no-fail-fast` in the
   debug profile.
3. **`full` is the landing gate.** It runs every `quick` step, then:
   `cargo test --offline --locked --workspace --no-fail-fast --release`
   with `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1` in the environment;
   `cargo clippy --offline --locked --workspace --all-targets`, and a
   `(lib)` warning count above the baseline for `subscript-compiler`,
   `subscript-runtime`, or `subscript-codegen` is a failure. The
   baseline is three integers at the top of the script, and it moves
   only by an owner decision that the tracking note records;
   `node_modules/.bin/tsc -p tsconfig.json`;

   **The count reads three `(lib)` lines and nothing else. Open.**
   *(Recorded 2026-09-11 by §106's round, which added a warning the
   gate could not see and then found two more already there.)* The
   command runs `--workspace --all-targets`, so clippy reports every
   target, and the `awk` filter keeps `subscript-compiler`,
   `subscript-runtime` and `subscript-codegen` `(lib)` only. A
   `(lib test)` warning, a `(test "…")` binary, a `(bin … test)`
   target, and every other crate — `subscript-cli`,
   `subscript-bindgen`, the benchmarks — pass unread. Measured at
   `495bb44`: `subscript-codegen (lib test)` carried 15 warnings and
   `subscript-cli` was never counted.

   Naming the missed targets does not converge as the workspace
   grows. The total form is a ceiling per target, or one ceiling over
   `--all-targets`, taken from the output the gate already has. A
   round that closes it measures every target's count first, because
   the baseline it proposes is that measurement and not a guess.
   `tools/hygiene.sh`.
   `full` does not stop at the first failure of a test command,
   because the record must hold every failing suite; it stops at a
   failed build.
4. **A skip is a declared fact.** A test that returns without its
   assertion because of the profile or an unset environment variable
   prints one line to stdout in this form:
   `gate-skip: <suite> <reason>`. The script counts these lines per
   test command. In `full`, the release run must print zero
   `gate-skip:` lines; one line is a failure. In `quick` the lines
   are listed, not failed. A test that skips by any other text is a
   defect of that test.
5. **The record.** Every run writes
   `target/gate/<UTC timestamp>-<shape>.md` and prints its path. The
   record holds, in this order: the shape; the UTC time; `git
   rev-parse HEAD`; the dirty state as the count and the list of
   `git status --porcelain` lines; the host triple; `rustc -V`,
   `cargo -V`, `node -v`, `tsc -v`, and the first line of `cc
   --version`; then one block per command with the exact command, the
   environment variables the script set, the wall seconds, the exit
   status, the sum of every `test result:` line as
   `passed/failed/ignored`, the count of `gate-skip:` lines and the
   lines themselves; then the list of pre-existing golden or
   `.expected` files that `git status --porcelain` reports modified
   or deleted under `corpus/`, `examples/`, and
   `codegen/tests/lir-goldens/` (`M`, `D`); then the verdict line.
   *(Corrected 2026-09-09 by the §95 Phase Review. The filter read
   `corpus/` alone, so §95's fourth moved golden,
   `examples/e07-determinism.expected`, was invisible and the verdict
   line said `goldens-moved 3` for four. `examples/tests/gate.rs`
   holds those goldens to the same dev ≡ ship ≡ golden rule.)* A run that a signal (`HUP`,
   `INT`, `TERM`) ends deletes its record and exits non-zero: there
   is no partial record. *(Added 2026-09-05 after review round 2: an
   interrupted run left a record with duplicated command blocks and
   no verdict.)*
6. **The verdict line** is one line, last in the record and last on
   stdout:
   `gate <shape> <rev> <clean|dirty:N> debug <p>/<f>/<i> [release <p>/<f>/<i>] skips <d>[/<r>] [clippy <c>/<r>/<g>] goldens-moved <m> exit <status>`.
   `<d>` is the count of `gate-skip:` lines of the debug test
   command and `<r>` that of the release test command. A bracketed
   field appears only when its step ran; a `full` run that stops at
   `fmt` or `build` shows no `release` and no `clippy` field. *(Amended
   2026-09-05, forced: the debug command declares the `perf_gate`
   skip by rule 4, so one total could not be 0 in `full`.)*
   The exit status is 0 only when every command exits 0, every test
   command reports 0 failed, and rule 4 holds. A moved golden does
   not change the exit status; the reviewer reads `goldens-moved`
   against the golden-change procedure (§2).
7a. **The dev profile keeps no loose object files.** *(Owner decision
   2026-09-06.)* The workspace `Cargo.toml` sets
   `[profile.dev] split-debuginfo = "off"` (the test profile
   inherits it). Measured 2026-09-06 (`specs/tracking/s85-gate-command.md`):
   with the macOS default `"unpacked"`, every link left its codegen
   unit objects in `target/debug/deps`, 707,606 of them after seven
   weeks, and a relinked test binary took 26–36 s to reach `main`;
   with the files gone, 0.64 s, and the debug gate step 543 s
   against 1,443–1,728 s. With `"off"` a fresh build leaves no
   `.rcgu.o`, the binary size is unchanged, and backtraces resolve
   through the rlibs' debug info. The change is not a move of the
   executables; the location was measured as irrelevant.
7. **The script owns no test.** It runs the commands above and reads
   their stdout. It sets no `CARGO_TARGET_DIR`, so a run measures the
   checkout it is in. `CARGO`, `NODE`, `TSC`, `CC`, and `GIT` are
   read from the environment with the defaults `cargo`, `node`,
   `node_modules/.bin/tsc`, `cc`, and `git`, so a test can substitute
   a stub. `GIT` serves every `git` call the script makes.

### 85.2 Sites

- `tools/gate.sh` (new): POSIX `sh`, the shape of `tools/hygiene.sh`.
- `codegen/tests/lir.rs` line 2020 area and
  `benchmarks/tests/perf_gate.rs` line 3 area: the skip text becomes
  the rule 4 form. `runtime/src/context.rs` and the benchmark
  binaries under `benchmarks/src/bin/` read `debug_assertions` for
  another purpose and do not change.
- `cli/tests/gate.rs` (new): the script's direct tests (core
  principle 1).
- §8.3 gains one line that names this section.

### 85.3 Corpus and gate (pre-registered exit criteria)

1. `cli/tests/gate.rs` runs the script with `CARGO` set to a stub
   `sh` script that the test writes. Each case compares the record
   and the verdict line against a **hand-written** expectation:
   (a) a stub whose `test` prints two `test result:` lines and one
   `gate-skip:` line, in `quick`: verdict `debug` sums the two lines,
   `skips 1`, `exit 0`;
   (b) the same stub in `full`: the release block's `gate-skip:` line
   makes `exit 1` (rule 4), and the record names the line;
   (c) a stub whose `build` writes `warning:` to stderr: `quick` exits
   1 at the build step and the record has no test block;
   (d) a stub whose `test` prints `1 failed`: `full` still runs the
   release step and `hygiene.sh`, and the verdict has `exit 1`;
   (e) a stub whose `clippy` prints a `(lib)` count one above the
   baseline for `subscript-codegen`: `exit 1`, and the verdict's
   `clippy` field shows the measured count;
   (f) an unknown shape argument: exit 2 and a usage line;
   (g) the plain stub in `full`, with no skip, no failure, and the
   baseline counts: `exit 0`, `skips 0/0`, `goldens-moved 0`;
   (h) a `GIT` stub whose `status --porcelain` prints
   ` M corpus/accept/x.expected`, `D  codegen/tests/lir-goldens/corpus.txt`,
   and ` M codegen/src/lib.rs`: `goldens-moved 2` and the two paths in
   the record, `exit 0`;
   (i) a stub whose `test` sleeps, and the test sends `TERM` to the
   script during that step: the exit status is non-zero, **the
   record the run reserved is absent**, and its scratch directory is
   gone. The case reads the record's path from the run's own output
   or from the one name that appeared under `target/gate/` after the
   spawn; it does not compare the whole directory against a snapshot.
   *(Corrected 2026-09-08, measured: the whole-directory comparison
   failed once inside a real `full` run, naming a record neither the
   case nor the enclosing run owned. `target/gate/` holds every run's
   records, and this case executes inside one, so a set comparison
   makes any concurrent writer a false failure. Rule 5's fact is
   about the run's own record.)* This case runs on a POSIX host
   only.
   *(Scoped 2026-09-06, measured on `x86_64-pc-windows-msvc`: a
   native parent starts the script, and the MSYS `kill` cannot map
   that Windows process id to a signal target. `kill -TERM`,
   `kill -W -TERM`, and `kill -f -TERM` each report `No such
   process`. `kill -W -f -TERM` ends the process through the Win32
   interface, so the trap does not run and the record stays. Windows
   has no delivery path for this case, so the test carries
   `#[cfg(unix)]`.)*
   (e) also covers the compiler and the runtime baselines, one stub
   variant each.
   Positive control for the record: case (a) also asserts the record
   file exists at the printed path and that the verdict line is its
   last line. Every case deletes the record it made before it
   returns, so `target/gate/` holds only real runs.
   **Every case runs with a `GIT` stub** whose `rev-parse HEAD`
   answers a fixed hash and whose `status --porcelain` answers the
   case's lines: empty for every case but (h). The expected verdict
   is then a literal string with no value read from the checkout.
   *(Added 2026-09-06, measured: the first §88 tree moved one golden,
   and nine cases that read the real `git status` reported
   `goldens-moved 1` against a hand-written `0`; a commit during a
   gate had failed one case the same way the day before. A test that
   reads the checkout's state is a test that the checkout can fail.)*
   `TSC` and `CC` point at stubs the test writes; `hygiene.sh` runs
   for real, on the checkout, because it reads the tree.
2. `cargo test -p subscript-codegen --test lir` in the release
   profile without the sweep variable prints exactly one `gate-skip:`
   line; `cargo test -p subscript-benchmarks --test perf_gate` in the
   debug profile prints exactly one. A test in each of the two files
   pins the exact line text by running the sibling test's skip path
   through a function that returns the line, compared against a
   hand-written string.
3. `tools/gate.sh full` at the landing revision, this host: the
   verdict shows `skips 1/0` (the debug `perf_gate` declaration, no
   release skip), `goldens-moved 0`, `exit 0`, and the
   passed counts are within the tracking note. Every pre-existing
   golden and `.expected` stays byte-identical.
4. Windows: `tools/gate.sh` runs under the `sh` that
   `tools/hygiene.sh` already requires. The record states the host
   triple, and the windows-portability note gains the verdict line
   when that host next runs. Case (i) of item 1 does not run there.
   The signal path of rule 5 stays unverified on Windows, and the
   windows-portability note records that fact.
