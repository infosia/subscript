# §90 — no public entry point panics or faults on any input

Status: **landed** at `155660d`. Contract: `specs/blocks/compiler.md` §90
(`3782747`). Origin: the owner's question of 2026-09-06.

## The measurement (at `68b4213`, this host)

Static, clippy on the lib and bin targets with `unwrap_used`,
`expect_used`, `panic`, `unreachable`, `todo`, `unimplemented`,
`indexing_slicing`, `arithmetic_side_effects`:

| crate | `panic!` | `unreachable!` | `unwrap` | `expect` | index/slice | arithmetic |
|---|---:|---:|---:|---:|---:|---:|
| compiler | 0 | 4 | 35 | 48 | 266 | 90 |
| runtime | 0 | 0 | 0 | 4 | 96 | 265 |
| codegen | 0 | 17 | 1 | 69 | 565 | 234 |
| bindgen | 0 | 0 | 0 | 4 | 22 | 25 |
| cli | 0 | 0 | 0 | 1 | 21 | 8 |

Every `unreachable!` and `expect` message names an internal
invariant ("validated above", "rejected above", "write to String",
"a synthetic prefix must have an owner"); 35 of the compiler's
`unwrap` are `write!` into a `String` in `lir_text.rs`.

Dynamic, release, one child process per input (a temporary example
binary, deleted): 590 `.ts` sources under `corpus/` and `examples/`,
each with 12 truncations at character boundaries, 12 single-byte
replacements, and up to 12 single-line deletions, through
`check_program` → `check_warnings` → `render_*` → `lower_module` →
`lir_text::print_module` → `emit_c`, and `interpreter::interpret`
for the unmodified entries; every `.h` under `corpus/interop/` and
`examples/` with 12 truncations and a garbage header through
`bindgen::generate`; 12 malformed argument lists through
`cli::execute`. **15,898 runs, 15,897 completed, 0 panics caught, 1
fault** (SIGSEGV): `a09-enums.ts` truncated to
`enum Status {\n  Ready`.

The fault: `swc_ecma_parser` 6.0.2 `parse_ts_enum_member`
(`src/parser/typescript.rs:788`) calls `bump!` in its error-recovery
branch with no current token. `bump!` is `debug_assert!(knows_cur)`
then `input.bump()`, whose `None` arm is `debug_unreachable!`: a
panic in debug ("parser should not call bump() without knowing
current token"), `unreachable_unchecked` in release. Measured:
`target/debug/subscript check` exit 101 with that message;
`target/release/subscript check` exit 139. `catch_unwind` around the
parser: debug catches; release faults the same way (the first probe
version, in-process, died at that input).

Excluded: the 213 runtime `extern "C"` functions (invariant 6;
§90.1 rule 5).

## Round 1 (at `68f2087`)

- The fork: `swc_ecma_parser` 6.0.2 as published (base commit
  `a0b54ed`) and one commit on branch `subscript-eof-bump`
  (`113e3c4`): `parse_ts_enum_member` tests `eof!` before `bump!` in
  its recovery branch and emits TS1005 at the end position; a
  regression test `tests/enum_eof.rs` registered from `src/lib.rs`.
  The patch is 21 lines. The fork's own test suite cannot run
  offline outside the workspace (dev-dependencies absent from the
  local registry); the test runs inside the workspace.
- r185 Red at `68f2087` with the registry crate: debug, 2 of 35
  reject tests fail at `typescript.rs:788:13` with "parser should
  not call bump() without knowing current token"; release, the test
  process dies with SIGSEGV. Green with the fork: 35 passed in both
  profiles; S100 at line 9; `tsc` rejects with TS1005.
- `compiler/tests/robustness.rs`: 421 sources (183 accept, 174
  reject, 5 warn, 53 trap, 6 interop), 14,985 mutated inputs
  through `check_program` under `catch_unwind`, 0 panics, 6.1 s in
  debug.
- The debug-profile repeat of the survey (this project's 19
  `debug_assert!` sites and swc's debug assertions live): 15,898
  inputs, 15,897 completed, 1 panic caught — the same swc site,
  `typescript.rs:788` — 0 faults, 1,783 s. No `debug_assert!` of
  this project fired.
- An incident, recorded: the coding agent's `git init` inside
  `target/forks/` was cut short by its sandbox (no `HEAD`), so a git
  command run in that directory resolved to the project repository;
  one commit landed on a stray branch of the project with the
  round's files. Reverted with `reset --soft` and the branch
  deleted; no history reached `main`. The fork repository now lives
  outside the project tree, and `target/forks/patches/` holds the
  `format-patch` of the fix.


## Landed at `155660d`

The fork is `https://github.com/infosia/swc_ecma_parser`, branch
`subscript-eof-bump`, pinned in `Cargo.lock` at `113e3c47` (base
`a0b54ed` = 6.0.2 as published). No other swc crate moved.

Gate:

```text
gate full d118fdc52bb493edc3dfd6f5bbe9c44226d44311 dirty:6 debug 1287/0/2 release 1285/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Step wall seconds: build 17, debug 383, release 499, clippy 9,
hygiene 1 — about 15 minutes for `full`.

## §90 result

Every public entry of the five crates survived 15,897 inputs in each
profile; the one fault was a third-party parser's, now a pinned
fork with a one-line fix, a reject entry, and a 15,000-input
mutation test in every gate. No production code of this project
changed.
