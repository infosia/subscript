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

## Named follow-ups (not dropped)

- `run --watch` hot reload — §2.5's own future contract revision.
- `emit-c` retirement decision — §2.2 leaves it to its own decision.
- READMEs present the CLI only briefly; a fuller "embedding a host"
  walkthrough remains undone.
