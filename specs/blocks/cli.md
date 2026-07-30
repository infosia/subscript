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
`subscript-cli`). Five subcommands; anything else is a usage error.
*(This line said "Four" until implementation counted the list —
corrected 2026-07-30; the list below was always the contract.)*

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
directory; never the CWD). `<name>` is the source file's stem plus
the platform executable suffix *(fixed 2026-07-30; the contract left
it undefined)*. `--run` executes the result, forwarding
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
crash. `--watch` (hot reload, compiler.md §8.2) is a named follow-up, **not**
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

The flags are accepted by the two runtime-consuming subcommands,
`link-flags` and `build` *(fixed 2026-07-30; the contract left
placement unstated)*. In order: explicit flags
(`--runtime-lib`, `--runtime-include`) →
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

## 8. Diagnostic rendering — Rev 2026-07-30

Owner decision. §2.1's original behaviour — silence on a clean check,
one line per rejection — under-serves both directions: success gives
no confirmation that anything ran, and an error names its position
without showing it. Diagnostics get a rich rendering; the data model
does not change.

### 8.1 The renderer

One public renderer in the compiler crate, colocated with `diag.rs`,
taking the program's `SourceFile`s and the diagnostics and returning
the rendered text. Every CLI path that prints a rejected program —
`check`, `emit`, `build`, and `run` — uses it, and prints
byte-identical text for the same rejected program. `emit-c` and the
corpus harnesses are untouched, as are `Diagnostic`'s fields and its
`Display` (the reject corpus asserts fields, never rendered text).

### 8.2 The shape, pinned

Per diagnostic (the rustc shape, single-caret because `Pos` is a
point, not a span):

```text
error[S007]: <message>
 --> <file>:<line>:<col>
  |
3 | const value: number = 1;
  |              ^
  = rule: <the code's one-line rule text>
```

- The gutter width follows the widest line number rendered.
- The `= rule:` line carries `RuleCode::explanation()` — a new
  method whose strings restate each code's contracted meaning
  (compiler.md §6); every code has one, and a test iterates the full
  enum.
- After all diagnostics, one summary line: `error: N error(s)`.
- **Degradation, never a panic:** a position whose file is not among
  the supplied sources or whose line is out of range renders the
  header and `-->` lines only, no snippet.
- Caret placement counts characters; earlier multi-byte or tab
  content may shift visual alignment in v1 — accepted, recorded
  here, not silently.
- No ANSI color in v1: output is byte-stable for tests. Color on a
  tty is a named follow-up, as is machine-readable `--json`.

### 8.3 Success is confirmed

A clean `check` prints exactly one line to stderr —
`check: <source as given>: no errors` — and nothing to stdout,
preserving §3 (stdout stays reserved for requested answers, keeping
`--json` open). Exit codes are unchanged (0 / 1 / 2).

### 8.4 Exit criteria (pre-registered)

1. Clean `check`: exactly the contracted stderr line, empty stdout,
   exit 0 — tested.
2. Exact-output tests pin: one diagnostic with snippet and caret; a
   multi-diagnostic program with its summary count; the degraded
   no-snippet path.
3. `check`, `emit`, `build`, `run` emit byte-identical rejection text
   for the same program — tested.
4. `RuleCode::explanation()` is non-empty for every code — tested
   over the full enum.
5. Full gate green; corpus accept/reject harnesses and
   `Diagnostic::Display` byte-untouched.

## 9. Multi-file programs — Rev 2026-07-30

Owner decision. The language has modules (`import { f } from
"./math"`; `corpus/accept/a19-modules/` pins them) but §2's
subcommands accepted exactly one source file, so an importing program
could not pass through the CLI at all. The CLI resolves imports from
disk.

### 9.1 Resolution

All four program subcommands (`check`, `emit`, `build`, `run`), given
the entry file, load its relative imports transitively: each
specifier resolves against the importing file's directory to
`<specifier>.ts`; each file loads once (paths normalized, so two
routes to one file do not duplicate it, and cycles terminate — cycle
*acceptance* stays the checker's question, not the loader's). What
counts as an import is decided by the compiler's own parse, exposed
as a library entry — the CLI performs no independent parsing of
source text.

A specifier whose file does not exist on disk is not a CLI error: the
loader passes what it found, and the checker's existing
"imported module … is not among the program's files" diagnostic
reports it with a position, rendered per §8. Mirrors are ambient and
unaffected.

The assembled file set must resolve exactly as the repository's
existing directory loading (`emit-c`, the gate harnesses) resolves
`a19-modules` — one program, one meaning, both front doors.

### 9.2 Exit criteria (pre-registered)

1. `subscript check corpus/accept/a19-modules/main.ts` exits 0 with
   the clean line.
2. `subscript run` on the same entry matches the committed a19
   golden byte-exact.
3. `subscript emit` on the same entry is byte-identical to `emit-c`'s
   directory-mode output for a19.
4. An import naming a missing file renders the checker's diagnostic
   with position, exit 1 — no panic, no bare IO error.
5. A two-file cycle terminates (loader loads each file once); its
   accept/reject outcome is whatever the checker already decides.
6. Single-file programs: behavior and output byte-unchanged.
7. Full gate green; no golden moves.
