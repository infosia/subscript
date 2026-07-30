# cli — evidence against specs/blocks/cli.md §6

Status: **landed and verified 2026-07-30.** Contract committed first
(2026-07-29); implementation followed; every §6 criterion below was
re-run by the reviewer, not taken from the implementer's report.

## What landed

New crate `cli/` (package `subscript-cli`, binary `subscript`):
`cli/src/lib.rs` (dispatch and the five subcommands, writer-injected
for tests), `cli/src/runtime_paths.rs` (§4 resolution with an injected
default-builder so tests never spawn cargo), `cli/tests/commands.rs`
(end-to-end clean and contracted-error paths per subcommand).

Shared entries factored rather than duplicated: `emit_c_files`
(`codegen/src/emit_files.rs`) is now the single filesystem emitter
behind both `subscript emit` and `emit-c`; the ship toolchain surface
(`host_c_compiler`, `add_c11_optimized_flags`,
`include_directory_arg`, `add_executable_output`,
`add_object_directory`, `runtime_system_libraries`,
`runtime_staticlib_name`, `CCompilerStyle` — a type replacing the
former `msvc: bool` parameters) is exported from `codegen/src/aot.rs`.
`subscript run` reuses the existing public `run_jit`.

Both capstone scripts are now wrappers with zero platform
conditionals, zero compiler flags, zero archive paths.

## §6 verification (reviewer-run, 2026-07-30)

1. Both capstones via their wrappers: exit 0, stdout byte-identical
   to `expected.txt` (`cmp`).
2. `subscript emit` vs `emit-c`, host capstone inputs (`--mirror`,
   `--no-entry`): output directories `diff -r` identical.
3. `subscript run` on `e01`–`e08`: all eight byte-identical to their
   `.expected`.
4. Wrapper grep for `uname`/case branches, `-std`/`fp:strict`,
   archive names, `kernel32`: zero hits.
5. `link-flags` prints the include argument, an archive path that
   exists after the in-repo build, and (per direct unit test) the
   Windows set `kernel32 ntdll userenv ws2_32 dbghelp`.
6. Resolution-order tests present and green: flags beat env beats
   in-repo default; outside-repo error names all three mechanisms.
7. Gate: 46 harnesses, 681 passed, 0 failed, exit 0, read directly.

## Contract corrections found by implementation

Recorded in the block spec where they belong: §2 said "Four"
subcommands over a list of five; `build`'s `<name>` was undefined
(now: source stem + platform suffix); §4 did not say which
subcommands accept the runtime flags (now: `link-flags` and `build`).

## §8 diagnostic rendering — landed and verified 2026-07-30

`render_diagnostics` (`compiler/src/diag_render.rs`) plus
`RuleCode::explanation()` (all 15 codes, mirroring the §6 doc
comments); `check`/`emit`/`build`/`run` rejection paths all print it.
Reviewer-run §8.4 evidence:

1. Clean `check`: exactly `check: <path>: no errors` on stderr, empty
   stdout, exit 0 (run on `examples/e01`).
2. S007 program rendered with header, `-->`, snippet, caret at the
   column, `= rule:` line, and `error: 1 error(s)` — reproduced
   locally, matching the pinned exact-output unit tests (single,
   multi-diagnostic with 2-wide gutter, degraded no-snippet).
3. `check` / `run` / `emit` rejection stderr `cmp`-identical for the
   same program (reviewer); the four-command identity test also
   covers `build`.
4. Explanation-per-code test iterates the full enum.
5. Gate: 46 harnesses, 686 passed, 0 failed, exit 0, read directly;
   `corpus_accept`/`corpus_reject` harness files untouched.

## warnings (specs/blocks/warnings.md) — landed and verified 2026-07-30

`WarnCode`/`Warning`/`check_warnings` (`compiler/src/warn.rs`, HIR
analysis, panic-free outside tests), `render_warnings` sharing the §8
shape with `render_diagnostics` byte-unchanged, new corpus arm
`corpus/warn/` (w01, w02) wired into the `tsc` gate, surfacing plus
`--deny-warnings` in all four subcommands. Reviewer-run §6 evidence:

1. w01 fires W001 at 15:26 and w02 fires W002 at 16:12, rendered as
   contracted, exit 0; `--deny-warnings` exits 1; `emit
   --deny-warnings` leaves no output directory.
2. Zero-warning net: corpus/accept (90 files) and examples (10)
   sweep clean in the harness; `e03-memory.ts` reproduced silent
   locally with the `no errors` line.
3. `check_program` signature and reject harness (12 tests,
   83 entries) unchanged and green.
4. Gate: 697 passed, 0 failed, exit 0, read directly; `tsc` gate
   exit 0 with `corpus/warn/**` included.

Implementer's recorded conservatisms: Map/Set count as reference
classes for W001; uncertain alias/reassignment identity does not
fire; the collect mute does not cross lambda/function boundaries;
W002 discards tracking at control-flow joins. All precision-first,
consistent with §2.

## §9 multi-file programs — landed and verified 2026-07-30

`cli/src/program_loader.rs` loads the entry's relative imports
transitively (BFS, canonicalized paths, one load each);
`parse_import_specifiers` (`compiler/src/parse.rs`) is the compiler's
own answer to "what does this file import" — the CLI scans no source
text. The loader chases only the shape the checker can resolve
(`./name`, no separators in the remainder): `../x` and `./sub/x` are
left unloaded so the checker's positioned S100 stays accurate — found
by reading `resolve_imports` (stem match, `check/mod.rs`), where the
first loader draft chased them into a misleading loaded-but-unmatched
state. Reviewer-run §9.2 evidence:

1. `check` on `a19-modules/main.ts`: clean line, exit 0.
2. `run` on the same entry: byte-identical to the committed
   `a19-modules.expected`.
3. `emit` vs `emit-c` directory mode: `program.c`/`.alloc.h`/`entry.c`
   all `cmp`-identical.
4. Missing import: positioned S100 rendering reproduced locally.
5. Cycle and parent/nested-import tests in the CLI suite (13 green).
6. Single-file behavior unchanged (`e01` output).
7. Gate: 47 harnesses, 705 passed, exit 0, read directly.

## §10 `subscript bind` — landed and verified 2026-07-30

The subcommand wraps the existing shared
`subscript_bindgen::generate_for_header`; bindgen sources and the
standalone binary are unchanged (59 standalone tests green). Output
is generated fully before any write, so a rejected header leaves no
partial mirror. Reviewer-run §10.2 evidence: stdout mode and
positional `-o` mode both `cmp`-identical to the committed engine
mirror; an unmappable `long` parameter exits 1 with the fail-loud
message naming the type and creates no output file; gate 47+1
harnesses, 708 passed, exit 0, read directly. Tutorial step 5 and the
README CLI section now show `subscript bind`.

## §11 retirements — landed and verified 2026-07-30

`emit-c` and the standalone `subscript-bindgen` binary are gone
(both `cargo run` attempts fail with no-bin-target); their CLI test
suites retired with them, which is the 708 → 702 gate delta.
`device-link.sh` step 1 now builds the CLI and emits from
`corpus/accept` with the bare entry name; the reviewer ran that step
against a pre-removal `emit-c` reference captured independently of
the implementer — all three files `cmp`-identical, so the gated
device halves (not runnable headlessly) consume byte-identical
input. The a19 §9.2 comparison re-anchors to in-process
`emit_c_files` with directory-mode file naming. Regeneration hints
in `examples/tests/gate.rs` and `bindgen/tests/regen.rs` name
`subscript bind`. Residual sweep outside specs/, docs/, and Cargo
manifests: zero references. Gate 702 passed, exit 0, read directly.

## §12 `run --watch` — landed and verified 2026-07-31

The watch cycle is a library state machine (`cli/src/watch.rs`
`step()` — Swapped/Refused/Diagnostics/Unchanged), unit-tested
without processes; the binary adds 150 ms mtime polling (file length
supplements mtime for rapid rewrites) and rendering. One codegen
addition: `ReloadSession::new_capturing_initializer_trap`, so an
initializer trap does not discard the live session. Reviewer-run
evidence, including a live driven session (start → body edit →
declaration edit → broken edit → fix → interrupt):

1. stdout carried program output only: `run 1..4` — the module
   counter survived every swap — with the edited body's value
   changing 10 → 77 → 99 → 55.
2. stderr: `watch: swapped`, `watch: refused: class DemoMarker`
   (declaration named), the §8-rendered S100 for the broken edit,
   `watch: waiting for a fix`, then `watch: swapped` on the fix.
3. The demo (`examples/hot-reload/`) checks clean under
   `--deny-warnings`, is enumerated by the `tsc` include and the
   zero-warning sweep (whose examples enumeration previously missed
   subdirectories — extended).
4. Gate 721 passed, exit 0, read directly; non-watch `run` tests
   byte-unchanged.

## Named follow-ups (not dropped)

- `run --watch` hot reload — §2.5's own future contract revision.
- `emit-c` retirement decision — §2.2 leaves it to its own decision.
- READMEs present the CLI only briefly; a fuller "embedding a host"
  walkthrough remains undone.
