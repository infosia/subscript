# §100 — the Windows host runs the standing gate

Contract: `specs/blocks/compiler.md` §100. Origin: §99.5 item 8 left
the Windows half of §99 to the owner. This note records that run and
the two defects it found.

Pin for every measurement: `39fb66077a28ad9b0f84a9980dda1bdb1bfb0958`.
Host `x86_64-pc-windows-msvc`, Windows 11 Pro 10.0.26200.
rustc 1.95.0 (59807616e), LLVM 22.1.2. MSVC `cl` 19.44.35222 for x64,
found by `cc::windows_registry::find_tool` (§11c).

## The build is green

| Command | Result |
|---|---|
| `cargo check --workspace --all-targets` | exit 0, 9.73 s |
| `cargo build --workspace --all-targets` | exit 0, 27.01 s |
| `cargo build --workspace --all-targets --release` | exit 0, 29.32 s |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --workspace --all-targets` | warnings only, 7/18/13 baseline |

The 7/18/13 clippy counts equal the counts every recorded gate line
carries, so clippy finds no Windows-only lint.

## The test run

`cargo test --workspace --no-fail-fast`: 1338 passed, 2 failed,
2 ignored. The macOS gate at `f6cde3d` records 1364 passed. The
difference is §11c constraint 2's structural exclusions, which compile
and run nothing on windows-msvc.

### §99's Windows half is discharged

`codegen/tests/long_string_constants.rs` passes 4 of 4 under MSVC `cl`,
in 73.51 s. A 1 MiB constant builds, links and runs on this host. This
is the run §99.5 item 8 asked for, and §99 needs no change.

## Red at the pin — defect 1, text-mode stdout

`codegen/tests/async_cleared_trap.rs:131`:

```
assertion `left == right` failed
  left: [109, 49, 13, 10, 109, 50, 13, 10]
 right: [109, 49, 10, 109, 50, 10]
```

`left` is `m1\r\nm2\r\n` and `right` is `m1\nm2\n`. The test builds its
own host program at line 50 and writes the captured sink with
`fwrite(output, 1, length, stdout)` at line 100. The MSVCRT opens
stdout in text mode, so it translates each `\n` to `\r\n`.

§11c names binary-mode stdout as a Windows detail of the `cl` path.
The production entry `AOT_ENTRY_C` carries the guard at
`codegen/src/ship.rs:83`. The `ship.rs` test module carries the same
guard in a private helper, `host_entry`, at `codegen/src/ship.rs:1534`.
An integration test cannot reach a private helper, so
`async_cleared_trap.rs` wrote a fresh host body without the guard.

This is §11c constraint 3's rule again, in a new place: a guard that a
test must copy is a guard that a test forgets.

Scope: `int main(void)` appears in two test files.
`codegen/tests/offsetof_layout.rs` is excluded on windows-msvc by
§11c constraint 2, and it reads its probe output with `lines()`.
`async_cleared_trap.rs` is the only site the run reaches.

## Red at the pin — defect 2, the host path separator

`codegen/tests/docs.rs:328`:

```
docs\tutorial-c-cpp.md: add the measured TypeScript fence count to the scope table
```

Line 319 spells the document path with `to_str()`, which gives the host
separator. The scope table at lines 324 to 327 spells each arm with
`/`. On Windows every arm below `README.md` misses, and the fallback
arm panics. `README.md` holds no separator, so the test reports it and
then stops.

The defect is a wrong spelling, not a wrong count. The counts 11, 3
and 17 are correct on this host.

The repository states this rule twice already, in two private helpers:

- `compiler/tests/tsc_corpus.rs:101` `repository_relative` joins the
  path components with `/`. Its comment names the reason.
- `compiler/src/language_reference.rs:846` `normalized_relative` does
  `.replace('\', "/")`.

`docs.rs` is the third site and it carries neither. A third instance of
one class is a defect of the form (CLAUDE.md, two-round limit).

`compiler/tests/js_corpus.rs:416` also keeps the host separator, but it
puts the string in a message only and compares nothing. The message
reads `corpus\accept\...` on Windows. It is cosmetic, and §100 covers
it for one spelling across the tree.

## Landed, 2026-09-09

Two implementation rounds. The first stopped before it changed a file,
and the stop was correct: §100.5 item 1 gave 1340 passed, which counted
the two repaired tests and forgot the three added ones. §100.5 items 1,
2 and 2a carry the corrected arithmetic.

The second round's review found one MAJOR. `host_entry` became `pub`
and kept its two `assert!` calls, so a public library function panicked
on its input. CLAUDE.md core principle 5 forbids that, and the helper is
not the FFI exception. It was the only `# Panics` section in
`codegen/src`, `compiler/src` and `runtime/src` together, so it had no
precedent. §100.2 rule 2 caused it: the rule said "fails" and named no
mechanism. Rules 2 and 2a now name `Result`.

One MINOR: three call sites in `compiler/src/language_reference.rs`
replaced the total `normalized_relative` with an `.expect`. Two now use
`?` with `invalid(...)`. The `sort_by_key` closure keeps its `expect`,
because a closure cannot return a `Result`.

### Verified here, not only reported

- `cargo test --workspace --no-fail-fast`: 1343 passed, 0 failed,
  2 ignored, exit 0. That is 1338, plus the two repaired tests, plus
  the three added ones.
- `cargo fmt --check` exit 0. `cargo build --workspace --all-targets
  --release` exit 0. `tools/hygiene.sh` exit 0.
- Clippy holds 7/18/13: compiler 7, runtime 18, codegen 13.
- The total check reports a real bypass. Removing the `host_entry`
  wrapper from `codegen/tests/async_cleared_trap.rs` gives
  `codegen/tests/async_cleared_trap.rs:51: §100.2: pass the C host body
  directly to host_entry before compilation`. The file was restored and
  the check passes again.
- Nothing under `corpus/`, `generated-docs/`, or any `.expected` moved.

### The check reads the tree, not a list

`codegen/tests/host_entry.rs` lexes Rust strings and comments before
punctuation, so a C brace does not close a Rust scope. Its negative
controls are a bare body, two bodies in one file, a body that escapes
`m` as `\u{6d}`, and bodies inside nested block comments. It exempts
two owners in `codegen/src/ship.rs` by name, `AOT_ENTRY_C` and
`host_entry`, and one literal in `codegen/tests/offsetof_layout.rs`,
which reads its probe output with `lines()` and which §11c constraint 2
excludes on windows-msvc.

## Second Windows run, 2026-09-10, at `a3007cf`

The Phase Review (`specs/tracking/phase-review-2026-09-09.md`) raised
one MAJOR against §100: the total check of rule 3 was not total. It
matched the byte string `int main(void)` and it read two directories,
so `int  main(void)` and `int main( void )` both bypassed it, and the
four hand-written host bodies in `benchmarks/src/bin/bound-call.rs`
were unread. The finding is correct. §100.2 item 3a and
`codegen/src/host_source.rs` close it.

`cargo test --workspace --no-fail-fast` on this host: 1359 passed,
**1 failed**, 2 ignored.

### Red — the helper's prologue lacks `<stdio.h>`

`codegen/tests/host_entry.rs:345`, the new
`c_definitions_share_recognition_and_compile`:

```
int  main(void): --- stdout ---
host.c(380): error C2065: 'stdout': undeclared identifier
```

The failure is in the helper, not in the test. `host_entry` injects
`(void)_setmode(_fileno(stdout), _O_BINARY);` and its prologue supplies
`HOST_HEADER_C`, `<fcntl.h>` and `<io.h>`. `HOST_HEADER_C` includes
`<stdint.h>` only. Nothing declares `stdout` or `_fileno`.

Every host body in the tree includes `<stdio.h>` itself, so the
translation unit compiled and the gap stayed invisible. The new check
compiles a bare body, which is what found it. Off Windows the injected
line is inside `#ifdef _WIN32`, so no other host reaches it.

Measured directly, `cl` 19.44.35222, `/nologo /std:c11 /utf-8 /Zs`,
on the prologue plus a bare `int  main(void)`:

| Source | Result |
|---|---|
| the prologue as shipped | `error C2065: 'stdout'`, exit 2 |
| the same plus `#include <stdio.h>` | exit 0 |

§100.2 rule 2b states the rule and rule 5 states the witness.

### Green, 2026-09-10

`<stdio.h>` joins the `_WIN32` block of `host_entry`'s prologue, first,
as `AOT_ENTRY_C` orders it. The prologue off Windows does not change.
One assertion in `host_entry_owns_the_guard_and_rejects_invalid_bodies`
moves to the new block text. No test is added or removed.

Verified here: `cargo test --workspace --no-fail-fast` 1360 passed,
0 failed, 2 ignored, exit 0. `cargo fmt --check` exit 0.
`cargo build --workspace --all-targets --release` exit 0.
`tools/hygiene.sh` exit 0. Clippy holds 7/18/13.

The Windows count is 1360 against the reference host's 1378. The
difference is §11c constraint 2's structural exclusions.
