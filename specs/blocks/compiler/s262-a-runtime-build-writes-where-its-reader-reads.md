<!-- §262 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 262. A runtime build writes where its reader reads

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R05.
Selected by the owner on 2026-11-13.

Problem, read at `7b39b207`. `cli/src/runtime_paths.rs` builds the
runtime with `cargo build --offline --release -p subscript-runtime` in
the repository root, and then reads
`<root>/target/release/<runtime archive>`. The command passes no
`--target-dir`. If `CARGO_TARGET_DIR` or `build.target-dir` names
another directory, `cargo` writes the archive there. The CLI then
reports a missing archive, or reads an old archive that an earlier
build left at the default path. The review measured the first result:
`link-flags` exited 2. §214 rule 1 fixed the same defect in the codegen
archive builder with `--target-dir`; it did not cover the CLI.

These scripts build with `cargo build` and then read a fixed path under
`target/`, with the same defect: `examples/host/build.sh`,
`examples/context-per-scene/build.sh`, `examples/hot-reload/run.sh`,
`codegen/device-link.sh`, and `spike/mobile-link/link.sh`.

### 262.1 Rules

1. A command in this repository that builds with `cargo` and then reads
   a file under the target directory passes `--target-dir` with the
   directory that it reads. The build and the read use one value.
2. The CLI passes `--target-dir <root>/target`, and reads
   `<root>/target/release/<runtime archive>`.
3. Each script of this repository passes `--target-dir` with the
   target directory that it reads (`"$REPO_ROOT/target"`, or the variable
   that the script sets to it) to each `cargo build` whose output it
   reads. A `cargo build` whose output the script does not read keeps
   the target directory of `cargo`, so a later `cargo` command of the
   same script uses the same one: `tools/gate.sh` builds, and then
   `cargo test` reuses that build.

### 262.2 Acceptance

1. Red first, at the pin: a unit test in `cli/src/runtime_paths.rs`
   reads the arguments of the build command, and finds no
   `--target-dir`.
2. After the change, the test finds `--target-dir` with
   `<root>/target`, and the lookup path is under the same directory. The
   build command is a value that the test reads with no `cargo` run, as
   in the §214 test of `runtime_build_command`.
3. A text test reads each `*.sh` file that git tracks, and finds
   `--target-dir` on each `cargo build` command, in each form: at the
   start of a line, after `cd … &&`, after `if` or `exec`, through a
   `CARGO` variable, and on a line continued with a backslash. A comment
   is not a command. The list of files is not fixed, so a new script is
   in the check. A command with no `--target-dir` is in an allowlist
   of file and reason: the script does not read its output. The test
   states its cost, and has a firing control for each form.
4. No corpus entry or golden changes.

### 262.3 Phase Review

One fresh review of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed: `spike/mobile-link/link.sh` builds three
   packages with no `--target-dir` and reads them under its
   `TARGET_DIR`. The problem list missed it.
2. MAJOR, realistic, fixed: the text test read a fixed list of four
   scripts, so it could not find a fifth. Acceptance item 3 now reads
   each `*.sh` file that git tracks.
3. MINOR, contrived, fixed: the test read only a line that starts with
   `cargo build`, so a `cd … && cargo build` command was not checked.
4. MINOR, fixed: the tracking note told the story of the round.

### 262.4 Open

None.
