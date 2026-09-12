<!-- §100 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 100. The Windows host runs the standing gate

*(2026-09-09.)* Origin: §99.5 item 8 left the Windows half of §99 to
the owner. The owner ran it. The measurements are in
`specs/tracking/s100-windows-gate.md`.

The run discharges §99's Windows half: `cl` 19.44.35222 builds, links
and runs a 1 MiB string constant, and
`codegen/tests/long_string_constants.rs` passes 4 of 4. §99 needs no
change.

The run also found two defects. Neither is in the compiler, the
runtime, or the emitted C. Both are in test code, and each one is a
rule the repository states elsewhere and does not carry here.

### 100.1 What the run measured

`cargo check`, `cargo build`, and `cargo build --release`, each over
the workspace with `--all-targets`, all exit 0. `cargo fmt --check`
exits 0. Clippy holds the 7/18/13 baseline, so no lint is
Windows-only.

`cargo test --workspace --no-fail-fast`: 1338 passed, 2 failed, 2
ignored. The recorded macOS count is 1364. The difference is §11c
constraint 2's structural exclusions.

### 100.2 Rule — a test host program obtains its entry from one helper

A test that compiles its own C host program, and then compares the
captured sink against expected bytes, must obtain the host source from
one shared helper. The helper prefixes the generated runtime header
and inserts the `_WIN32`-guarded `_setmode(_fileno(stdout), _O_BINARY)`
that §11c requires. No test writes that guard itself.

1. The helper is public in `subscript-codegen`, because an integration
   test cannot reach a private test-module item. The existing private
   `host_entry` in `codegen/src/ship.rs` becomes that helper, and the
   `ship.rs` test module calls it.
2. The helper returns a `Result`. It fails when the body declares no
   `int main(void)`, and it fails when the body already spells
   `_setmode`. One place owns the guard. *(Corrected 2026-09-09: this
   rule first said "fails" and named no mechanism. The first
   implementation read that as `assert!`, which made a public library
   function panic on its input. CLAUDE.md core principle 5 gives the
   mechanism: no panics in library code, `Result` and `?`. The FFI
   boundary is the single exception, and this helper is not it.)*
2a. A test call site unwraps that `Result`. A panic in a test is the
   correct failure, and it is not library code. The test of item 4
   below then asserts an `Err`, not a caught unwind.
2b. **The helper's prologue carries the dependencies of the code the
   helper injects.** *(2026-09-10, measured on windows-msvc.)* The
   injected statement reads `stdout` and calls `_fileno`, which
   `<stdio.h>` declares. The prologue supplied `<fcntl.h>` and
   `<io.h>` and not `<stdio.h>`, so the helper produced a translation
   unit that does not compile unless the body itself includes
   `<stdio.h>`. Every host body in the tree does, so the gap stayed
   invisible until item 3a's check compiled a bare body. Off Windows
   the injected line is inside `#ifdef _WIN32`, so only the MSVC path
   reaches it.

   Measured with `cl` 19.44.35222, `/std:c11 /Zs`: the prologue as
   shipped gives `error C2065: 'stdout': undeclared identifier` and
   exit 2; the same source with `<stdio.h>` added exits 0.

   `<stdio.h>` joins the `_WIN32` block, beside `<fcntl.h>` and
   `<io.h>`. The prologue off Windows does not change, because the
   dependency does not exist there. A body that already includes
   `<stdio.h>` is unaffected: the header is idempotent.

   The rule is general. A helper that injects code owns that code's
   includes. A body supplies only what the body itself uses.
3. A build-time check reports every remaining site at once. It reads
   the test sources and it fails when a host body reaches a C compiler
   without the helper. A per-site fix does not converge (CLAUDE.md).

3a. *(Corrected 2026-09-09 by the Phase Review.)* The check as first
   written was not total. It matched the exact byte string
   `int main(void)` and it read two directories. Measured bypasses,
   each of which compiles as C and each of which the check and the
   helper both miss: `int  main(void)` with two spaces, and
   `int main( void )`. `benchmarks/src/bin/bound-call.rs` already
   holds four hand-written host bodies that reach `host_c_compiler`
   and no check reads them, and `examples/` carries the crate as a
   dev-dependency, so a host body there is unread too.

   The check matches a C function definition of `main`, not one
   spelling of it, and it reads every Rust source in the workspace.

   A type cannot make this class unreachable, because a test can
   write any bytes to any file. §11c constraint 3's type is not
   available here. This is the total check the form admits, and the
   rule says so rather than claiming more.
4. This is §11c constraint 3's rule in a new place: a guard that a test
   must copy is a guard that a test forgets. §11c carried it into the
   native-library helper; §100 carries it into the host entry.
5. A test compiles the helper's output for a body that includes
   nothing, on every host. That is the witness for rule 2b, and it is
   what found the defect.

Measured at the pin: `codegen/tests/async_cleared_trap.rs` compares
`m1\r\nm2\r\n` against `m1\nm2\n` and fails.

### 100.3 Rule — a repository-relative path is spelled with `/`

A test or a generator that turns an absolute path into a
repository-relative name must spell that name with `/` on every host.
The name is data: a table arm matches it, a message prints it, and a
generated document embeds it. A host separator makes one host's name
different, and the comparison then fails on that host alone.

1. One helper produces the spelling. It joins the path components with
   `/`. It never returns the host separator.
2. Every site uses it: a scope table, a corpus walk, an error message,
   and a generated document. The three private helpers that exist
   today — `compiler/tests/tsc_corpus.rs`, `compiler/src/language_reference.rs`,
   and the site that lacks one — collapse to it.
3. A `/`-spelled literal in a match arm or an assertion is correct. The
   rule moves the path to the literal, never the literal to the path.

Measured at the pin: `codegen/tests/docs.rs` reads
`docs\tutorial-c-cpp.md`, misses every scope-table arm, and panics.

### 100.4 Sites

- `codegen/src/ship.rs`: `host_entry` becomes public and documented.
- `codegen/tests/async_cleared_trap.rs`: its host body goes through
  the helper.
- `codegen/tests/docs.rs`: line 319 spells the name with `/`.
- `compiler/tests/tsc_corpus.rs` `repository_relative`,
  `compiler/src/language_reference.rs` `normalized_relative`, and
  `compiler/tests/js_corpus.rs` line 416: one spelling.
- The total check of 100.2 rule 3.

### 100.5 Gate (pre-registered exit criteria)

Red first, at the pin `39fb660`: the two failures above, on
`x86_64-pc-windows-msvc`.

Items 3 to 5 add three tests, and those three run on every host. The
counts below carry them. *(Corrected 2026-09-09: this section first
gave 1340 for item 1 and "no movement" for item 2. Both figures counted
the two repaired tests and forgot the three added ones. The coding
agent reported the contradiction and stopped, which is the wanted
outcome.)*

1. `cargo test --workspace` on windows-msvc: 0 failed, and 1343
   passed. That is the pin's 1338, plus the two tests this section
   repairs, plus the three it adds.
2. Off Windows the count grows by the same three. The recorded macOS
   1364 becomes 1367, and 0 fail. Both rules are no-ops off Windows,
   so no golden moves and the LIR snapshot does not move.
2a. If the implementation needs a different number of tests, it reports
   the number and the reason, and it does not adjust a count to fit.
3. The total check of 100.2 rule 3 fails on a host body that skips the
   helper. A test builds that body and asserts the failure.
4. A test asserts that the helper returns `Err` for a body with no
   `int main(void)`, and for a body that already spells `_setmode`.
   It uses no `catch_unwind`.
5. A test asserts that the path helper returns a `/` spelling for a
   path built with the host separator.
6. `tools/gate.sh full` green in both profiles on the reference host.
   `cargo fmt --check` green. Clippy at the 7/18/13 baseline.
