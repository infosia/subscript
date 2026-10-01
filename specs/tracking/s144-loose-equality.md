# §144 loose equality

Contract: `specs/blocks/compiler/s144-loose-equality-is-strict-equality.md` at `87e9a7b7`.

Red: the CLI built at that pin rejects a307 with exit code 1 and 16 S100 diagnostics.
Each says: "loose equality is not in the language; use `===` / `!==`".
The final line says: "error: 16 error(s)".

All four equality operators use one checker path and retain the source operator in equality diagnostics.
The change removes `Divergence::LooseEquality`, its entry and test, r294, its reject-test row, and all C20 matrix records.
The corpus-index generator adds a307 and removes r294.
Loose matrix forms use the strict forms' concrete-instance conditions.
The product's omitted-instance count changes from 7473 to 7719 after the C20 waiver removal.

Updated exact message expectations:

| Test | Old | New |
| --- | --- | --- |
| `language::array_and_regexp_identity_keep_the_s100_acceptance_boundary` (array) | "operator not defined for `i32[]` and `i32[]`" | "operator `===` not defined for `i32[]` and `i32[]`" |
| Same test (RegExp) | "operator not defined for `RegExp` and `RegExp`" | "operator `===` not defined for `RegExp` and `RegExp`" |
| `composite_constraints::equality_diagnostics_name_declared_parameters` | "operator not defined for `T` and `U`" | "operator `===` not defined for `T` and `U`" |

Unit tests compare verdicts, codes, and operator-specific messages for 26 operand pairs and both equality families.
Separate tests cover descriptor presence and rejected ordinary `undefined` operands.

Node v24.18.0 with TypeScript 5.9.2, dev JIT, and ship C AOT each produce these exact bytes:

```text
true true
true true
true true
true true
true true
true true
true true
true true
true true
```

Each output equals the a307 golden: 90 bytes, with nine final-newline lines.
The a307 codegen corpus probe passes on a source and runtime archive copy under `$TMPDIR` (0.56 seconds).
Typed comparison functions prevent TS2367 for the boolean and enum constants; measured tsc accepts a307.
No other golden changes.
`cargo test --offline --locked -p subscript-compiler` passes, including the 361 library tests and all integration tests.
`cargo fmt --check` and `git diff --check` pass. `tools/gate.sh` does not run.
The r294 deletion remains unstaged. No commit occurs.

Changed Rust file sizes (lines):
`operator.rs` 1522; `divergence.rs` 1307; `tests/mod.rs` 10; `tests/equality.rs` 190; `tests/language.rs` 1614;
`composite_constraints.rs` 143; `corpus_reject.rs` 1365; `generic_tsc_matrix.rs` 1834; `generic_tsc_matrix/product.rs` 572.
Other changed sizes: a307 source 37; golden 9; corpus index 721.

## Landing gate

```text
gate full 8006cf620f6b600663ba67098697d381dd411ef4 dirty:15 debug 2182/0/3 release 2179/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
