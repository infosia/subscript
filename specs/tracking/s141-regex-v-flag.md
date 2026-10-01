# §141: The `v` flag in a regex literal

Contract: `specs/blocks/compiler/s141-a-regex-literal-has-no-v-flag.md` at `70fd5039`.

## Red

The CLI was built from the contract pin before the checker changed.
The new entry is `corpus/reject/r289-regex-literal-v-flag.ts`.

Commands:

```sh
cargo build --offline --locked -p subscript-cli
target/debug/subscript check corpus/reject/r289-regex-literal-v-flag.ts
target/debug/subscript run corpus/reject/r289-regex-literal-v-flag.ts
cargo test --offline --locked -p subscript-compiler --test corpus_reject every_reject_entry_fails_with_its_rule_code_at_the_offending_line -- --exact
```

Measured output:

```text
check: corpus/reject/r289-regex-literal-v-flag.ts: no errors
true
r289-regex-literal-v-flag.ts was accepted; expected S100
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 38 filtered out
```

The check and run commands exited 0. The reject test exited 101.

TypeScript 5.9.2 used the project's compiler options and prelude in a temporary project.
Only the new reject entry was included.

```text
corpus/reject/r289-regex-literal-v-flag.ts(8,28): error TS1501: This regular expression flag is only available when targeting 'es2024' or later.
tsc exit: 2
```

## Change

The checker rejects a literal whose AST flags contain `v` before regex validation or global creation.
The diagnostic is S100 at the literal. The message names ES2024 and `new RegExp(pattern, "v")`.
The constructor path does not change.

The flag-set accessor tests use a constructor for sets that contain `v`.
The tests still cover all previously accepted flag sets.

## Verification

Checker tests require exactly one diagnostic for `/a/v` and `/a/gv`, with the full message and literal position.
Control tests accept `/a/u` and `new RegExp("a", "v")`.
The interpreter, dev JIT, and ship C controls require `true u` and `true v` output.
No existing expected golden changes.

The focused compiler tests passed: 39 reject tests, two literal-flag tests, and two accessor tests.
Both codegen accessor tests passed in 0.52 seconds, with one ship-C program compile.
The control output was `true u\ntrue v\n` on all three engines.
TypeScript 5.9.2 accepted the controls with the project's compiler options and prelude; exit 0.
The document generator added the reject entry to `generated-docs/corpus-index.md`.
The pinned `cargo fmt --check` passed. The all-target workspace build passed with zero warnings.
Clippy exited 0 with no diagnostics in changed files. Compiler/runtime/codegen library warning counts were 3/18/13.
Full gate: `gate full 70fd503930463c8275e194503a4acbd1d7268d89 dirty:8 debug 2131/0/3 release 2128/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0`.

Phase Review (Opus): no CRITICAL or MAJOR. One contrived MINOR, recorded: `"aa".replaceAll(/a/v, "b")` gives two diagnostics (the replaceAll-needs-`g` S100 and the `v` S100); `tsc` gives TS1501 only. Both diagnostics are correct.
