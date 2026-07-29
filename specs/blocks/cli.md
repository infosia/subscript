# cli — the `subscript` developer command

Status: contracted 2026-07-29 (owner decision); implementation follows
this contract. Evidence lands in `specs/tracking/cli.md`.

## 1. Why this exists, and its boundary

The capstone build scripts (`examples/host/build.sh`,
`examples/context-per-scene/build.sh`) each re-encode the same
knowledge: `emit-c` argument conventions, the per-OS runtime archive
name, the Windows system-library set, MSVC discovery through the §11c
shim, and the contracted C flags. Any host developer without the CLI
re-invents that script. The CLI owns that knowledge once.

**Boundary.** subscript is embedded: the host's build system owns the
final build. The CLI's upper bound is therefore *answers and
artifacts* — emit the C, name what to link, and as a convenience build
a small host whole. It does not generate host build-system files, does
not watch the host's sources, and does not wrap or replace the host's
compiler for the host's own translation units beyond the one-shot
`build` convenience.

## 2. Surface

One binary, `subscript`, in a new top-level crate `cli/` (package
`subscript-cli`). Four subcommands; anything else is a usage error.

### 2.1 `subscript check <file.ts> [--mirror <file.d.ts>]...`

Front-end only: parse + type-check, print diagnostics, exit 0 on
clean, 1 on diagnostics, 2 on usage/IO error. No artifact.

### 2.2 `subscript emit <file.ts> -o <dir> [--mirror <file.d.ts>]... [--no-entry]`

Emits `program.c` (and its companions exactly as `emit-c` does today)
into `<dir>`. The output is **byte-identical** to `emit-c` for the
same inputs — both front doors call the same library entry; `emit-c`
remains for the existing gates and may later be retired by its own
decision, not silently by this one.

### 2.3 `subscript link-flags [--cc <style>]`

Prints, one per line, what a host build must add to link the emitted
C: the runtime include directory, the runtime static archive path,
and the platform system libraries (`kernel32 ntdll userenv ws2_32
dbghelp` on Windows, none elsewhere). `--cc` selects flag spelling
(`unix` default, `msvc`). Paths resolve per §4. Exit 2 if the archive
cannot be resolved; never guesses.

### 2.4 `subscript build --source <file.ts> [--mirror <d.ts>]... [--host <file.c>]... [-o <dir>] [--run]`

The one-shot convenience: emit + compile + link the emitted C, the
given host sources, and the runtime archive into `<dir>/<name>`
(default `<dir>` = a `subscript-build` directory under the source's
directory; never the CWD). `--run` executes the result, forwarding
its exit code and leaving stdout untouched (compiler chatter goes to
stderr, as the scripts do today). Include directories: each `--host`
file's directory, each mirror's directory, and the runtime include
dir.

Toolchain selection is §11b/§11c and is not restated here: `$CC` if
set, else platform default (`clang`; MSVC `cl` through the existing
shim on Windows), with the §11c flag set (`/fp:strict`, joined-path
`-Fo`/`-Fe`) and the Unix contracted flags
(`-std=c11 -O2 -fwrapv -ffp-contract=off`) exactly as
`codegen/src/aot.rs` already encodes them — reused, not duplicated.

### 2.5 `subscript run <file.ts>` — dev tier

Runs the program under the dev JIT and prints its output. v1 scope:
programs without host C bindings (the class the JIT gate already runs
standalone). A program needing host symbols is a clear error, not a
crash. `--watch` (hot reload, §8.2) is a named follow-up, **not**
dropped: it is the dev tier's reason to exist and gets its own
contract revision when taken.

## 3. Behaviour rules

- Errors are single clear messages with exit 2 (usage/environment) or
  1 (program diagnostics); no panics (CLAUDE.md rule 5 applies — the
  CLI is library code plus a `main`).
- All work is offline and headless; the CLI never fetches anything.
- Output files land only under `-o`; nothing is written to the CWD.
- stdout carries only program output (`run`, `build --run`) or the
  requested answer (`link-flags`); everything else is stderr.

## 4. Path resolution (runtime archive and include dir)

In order: explicit flags (`--runtime-lib`, `--runtime-include`) →
environment (`SUBSCRIPT_RUNTIME_LIB`, `SUBSCRIPT_RUNTIME_INCLUDE`) →
in-repo default (`target/release/` archive per platform name,
`runtime/include/`), building the archive via
`cargo build --offline --release -p subscript-runtime` only in the
in-repo case. Outside the repo with no flag and no env: exit 2 with a
message naming all three mechanisms.

## 5. What the scripts become

Both capstone scripts shrink to thin wrappers — argument lines and a
`subscript build --run` invocation, no platform branches, no
compiler flags, no archive paths. The knowledge they carried lives
only in the CLI (and §11b/§11c) afterward. The scripts stay, because
they document each capstone's exact inputs and remain the committed
way to run them.

## 6. Exit criteria (pre-registered)

1. `subscript build --run` reproduces both capstones' stdout
   byte-identical to their committed `.expected`, headless, offline.
2. `subscript emit` output is byte-identical to `emit-c` for the
   host capstone's inputs.
3. `subscript run` on `examples/e01`–`e08` matches each `.expected`
   byte-exact (same outputs the examples gate already pins).
4. Both build.sh files contain zero platform conditionals and zero
   compiler flags.
5. `link-flags` names an archive that exists after an in-repo build,
   and its Windows system-library set equals the one the scripts
   carried.
6. Unit tests cover: each subcommand's clean path, each contracted
   error exit, and the §4 resolution order (flags beat env beats
   default).
7. Full workspace gate green (`tsc`-clean included); no golden moves.

## 7. Out of scope (this contract)

`--watch` hot reload (named follow-up, §2.5); packaging/installers;
cross-compilation targets beyond the host platform; generating host
build-system files; any change to `emit-c`, `emit-object`, `capture`,
or `msvc-cl` beyond factoring shared library entries.
