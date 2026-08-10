# §54 — caller link inputs follow the translation units

Status: **landed 2026-08-10** against `specs/blocks/compiler.md`
§54. Origin: a downstream report. Contract `7f9ce6f`,
implementation `585e073`.

## The report

A downstream host supplies a static archive in the `c_sources`
slot. Its dev-JIT tier passed with 0 failures. Its ship C-AOT
tier failed 12 tests on Linux, each with an undefined reference
to a symbol that the archive defines.

## Findings on this host, before the contract

- `add_native_compile_inputs` added the include arguments and the
  caller's link inputs together, and both call sites placed that
  group before `program.c` and `entry.c`.
- GNU `ld` keeps an archive member only against a symbol that is
  undefined at the position of the archive. At that position no
  symbol is undefined. `ld` keeps no member, and the link fails on
  every symbol the program calls.
- Measured 2026-08-10 (clang 14.0.0, GNU ld 2.38): one archive
  that defines `libProbe`, one caller that calls it. The archive
  before the caller fails with `undefined reference to
  `libProbe``. The archive after the caller links and runs. The
  position is the only variable.
- The defect is Linux-only. Apple `ld64` and MSVC `link.exe`
  resolve an archive independent of its position.
- The tests missed it for one reason: the only `c_sources`
  coverage passed three `.c` files. The driver compiles a `.c`
  file into an object, and an object contributes its symbols at
  any position. No test passed an archive.

## What landed

`add_native_compile_inputs` splits into
`add_native_include_directories` and `add_native_link_inputs`.
The include call keeps its position, because an include argument
is position-independent. Both AOT link commands order their
inputs: translation units, caller `c_sources`, runtime archive,
system libraries. No `--start-group`: the caller controls the
order inside `c_sources`.

A new dev-only crate, `codegen/tests/archive-fixture`, builds a
static archive that defines `subArchiveOnlyProbe` and nothing
else (confirmed with `nm`). It spells no `_Float16`, so it is
ungated and the MSVC gate keeps it. Its build script exports the
archive path and the header directory to the test through
`cargo:rustc-env`, so no path is committed.

The new test in `codegen/tests/native_library.rs` passes that
archive alone in the `c_sources` slot, and runs one program that
calls the archive symbol through ship C-AOT, through the retained
Cranelift-object AOT, and through the dev JIT. All three produce
the same bytes.

## Red, measured at `1993578`

`codegen/src/aot.rs` at `1993578`, the new test in place:

```
thread 'static_archive_link_input_follows_translation_units_on_all_tiers'
panicked at codegen/tests/native_library.rs:132:10:
ship C-AOT tier runs with the static archive: Internal(
"internal lowering error: compiling/linking the emitted C failed:
--- stderr ---
/usr/bin/ld: /tmp/program-89ed4e.o: in function `subscript_export_main':
program.c:(.text+0x1a): undefined reference to `subArchiveOnlyProbe'
clang: error: linker command failed with exit code 1")
test result: FAILED. 0 passed; 1 failed
```

## Gates (this host, at `585e073`)

- `cargo test -p subscript-codegen --test native_library`: 7
  passed, 0 failed.
- `cargo test --offline --workspace --release`: 925 passed, 0
  failed, 1 ignored, 1 filtered, exit 0.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0.
- Golden ledger: 192 `.expected` files, none modified. The change
  moves link inputs; it computes nothing.

**One gate did not run.** `codegen/tests/api_reference.rs` starts
`node`, and this host resolves no `node`, `nodejs`, `bun`, `deno`,
or `qjs`. The `tsc` gate needs the same toolchain and did not run
either. The filtered test is the count difference against the
`432b3b3` record of 925 passed: that run included the witness
test and not the new one. `api_reference.rs` is unchanged by this
work.

## Not verified here

macOS and windows-msvc ran no test for this change. Both
platforms resolve an archive independent of its position, so the
order change cannot regress them. The fixture crate is ungated,
so the next Windows gate run will build the archive with `cl`.

## macOS confirmation — 2026-08-10

Measured on the arm64 macOS reference machine at `a94eeb0`, with
the pinned toolchain:

- `cargo test --offline --release -p subscript-codegen --test
  native_library`: 7 passed, 0 failed, exit 0. The archive test
  passes; `ld64` resolves an archive at any position, so this run
  checks for a regression from the reorder and finds none.
- Full workspace release gate: 926 passed, 0 failed, 1 ignored,
  exit 0. The count includes the new archive test and the
  `api_reference` witness test, which this host runs (`node`
  resolves here). That closes one of the two gates the Linux host
  could not run.
- `tsc` gate: exit 0. That closes the other.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
  `cargo fmt --check`: exit 0.

Open: the windows-msvc gate has not run this change. The fixture
crate is ungated, so the next Windows gate run builds the archive
with `cl`.
