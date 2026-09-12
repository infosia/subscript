<!-- §90 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 90. No public entry point panics or faults on any input

*(Owner decision 2026-09-06.)* Origin: the owner's question "does
any API panic". Contract for every public entry of the four library
crates and the CLI: `check_program`, `check_warnings`,
`render_diagnostics`, `render_warnings`, `lir_text::print_module`,
`lower_module`, `emit_c`, `interpreter::interpret`, `run_jit*`,
`run_c_aot*`, `bindgen::generate`, `cli::execute`, and the `Context`
methods the host calls.

Measured at `68b4213`, this host. Static: the library crates hold no
`panic!`; 21 `unreachable!` and 126 `expect` sites, every one an
internal invariant ("validated above", "write to String"); 970
index or slice sites; 622 arithmetic sites that the debug profile
checks. Dynamic, release profile, one child process per input:
every `.ts` under `corpus/` and `examples/` (590 sources) and, for
each, 12 truncations at character boundaries, 12 single-byte
replacements, and up to 12 single-line deletions, through
`check_program` → warnings → `lower_module` → `print_module` →
`emit_c` (and the interpreter for the unmodified entries); every
`.h` under `corpus/interop/` and `examples/` with 12 truncations and
a garbage header through `bindgen::generate`; 12 malformed argument
lists through `cli::execute`. 15,898 runs: 15,897 completed, 0
panics, **1 fault**: `corpus/accept/a09-enums.ts` truncated inside
an enum member (`enum Status {\n  Ready` then EOF).
`swc_ecma_parser` 6.0.2 `parse_ts_enum_member`
(`src/parser/typescript.rs` line 788) calls `bump!` in its
error-recovery branch with no current token; `bump!` is
`debug_assert!` then `input.bump()`, which is `debug_unreachable!`
on `None` — a panic in the debug profile
("parser should not call bump() without knowing current token") and
`unreachable_unchecked` in release, measured as SIGSEGV from
`target/release/subscript check`. `catch_unwind` cannot contain it:
in release it is not a panic.

### 90.1 Rule

1. **A public entry returns.** On any byte sequence, any argument
   list, and any well-typed value, a public entry of this table
   returns its `Result` or its exit code. A parse failure is an S100
   diagnostic. A panic reaching a public entry is a defect of this
   section; so is a fault.
2. **The parser is a fork, pinned.** `swc_ecma_parser` is the
   project's fork of 6.0.2 with one change: in
   `parse_ts_enum_member`, the error-recovery branch tests `eof!`
   before `bump!`, and at EOF reports the syntax error at the end
   position without consuming. The fork carries the crate's own
   regression test for that input. Declared as `regress` is
   (`git = <URL>, branch = …`), pinned by commit in `Cargo.lock`
   (CLAUDE.md, non-goals: forks, no upstreaming). No other swc crate
   moves.
3. **The mutation survey is a standing test.**
   `compiler/tests/robustness.rs` runs the same mutation set as the
   measurement (truncation, byte replacement, line deletion; 12 each)
   over every accept, reject, warn, trap, and interop entry through
   `check_program` in-process under `catch_unwind`, and asserts zero
   panics. It runs in the debug profile, where a swc debug assertion
   is a panic and this project's `debug_assert!` sites are live; the
   release form of the same defect is UB that no in-process test can
   catch, so the corpus reject entry of item 4 is the release
   witness. The test lists every panic with its location and the
   input label before it fails.
4. **The corpus pins the shape.**
   `corpus/reject/r185-enum-member-at-eof.ts`: `enum Status {\n  Ready`
   with no closing text; `expected-error: S100 at the enum member`;
   `tsc: rejects` (measured). Red at `68b4213` on both profiles: the
   debug checker panics, the release checker faults.
5. **The runtime FFI is outside this section.** Generated code and
   the host call the 213 `extern "C"` functions under the shared
   contract (invariant 6: scripts are trusted); a panic there aborts
   (Rust ≥ 1.81 `extern "C"` unwind → abort) and never crosses the C
   boundary.

### 90.2 Sites

- The fork repository (owner-hosted, cited by URL in `Cargo.toml`);
  `compiler/Cargo.toml`, `Cargo.lock`.
- `compiler/tests/robustness.rs` (new), `corpus/reject/r185-…`,
  `compiler/tests/corpus_reject.rs` (the row),
  `generated-docs/corpus-index.md` (regenerated).

### 90.3 Corpus and gate (pre-registered exit criteria)

1. Red: r185 panics the debug checker and faults the release checker
   at `68b4213` (measured above); the fork's own test fails on the
   unpatched crate.
2. Green: r185 is S100 on both profiles; the robustness test passes
   in debug with 0 panics over the mutation set (count reported);
   `tools/gate.sh full` green, `goldens-moved 0`.
3. The survey's other 15,897 inputs stay green (the robustness test
   is that survey, minus the interpreter and the CLI cases, which
   the corpus suites and `cli/tests` cover).
