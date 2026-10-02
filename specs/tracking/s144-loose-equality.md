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

## Phase Review fix round 3

`Divergence::ReferenceSearchMiss` records C22 with a table entry and a unit test.
a308 tests reference-element `Map.get` and `find` misses with `== null`, plus both hits.
Node v24.18.0 with TypeScript 5.9.2, dev JIT, and ship C AOT each match the 26-byte golden.
The temporary a308 corpus probe passes on both tiers in 0.90 seconds.
The HIR comment, descriptor-read diagnostic, and language-reference generator name all four equality operators.
Updated diagnostic-fragment tests: `language::absence_capable_member_read_in_absent_arm_is_rejected` and `language::absence_capable_member_reassignment_invalidates_narrowing`.
The generator refreshes the language reference and corpus index.
`cargo test --offline --locked -p subscript-compiler` passes, including the JS corpus, measured tsc headers, and generated-docs checks.
`cargo fmt --check` and `git diff --check` pass. `tools/gate.sh` does not run. No commit occurs.

## Phase Review fix round landing gate

The tree also held the teaching-material change (examples and docs write
`==`/`!=`, `examples.md` §2).

```text
gate full 6f435252053fee5b8601c20ea2c4e32359875408 dirty:24 debug 2183/0/3 release 2180/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review fix round 4

The q24-map-get-miss witness uses an inferred binding and `=== null` on both sides.
TypeScript 5.9.2 accepts its subscript source; the checker accepts it through the API-reference execution test.
Dev JIT prints `true`; Node v24.18.0 prints `false`.
a309 tests `done` and `value == null` after a `Generator<Box | null>` finishes.
Node, dev JIT, and ship C AOT each match its 10-byte golden: `true\ntrue\n`.
The C22 divergence reason includes finished nullable-reference generators.
The generator refreshes the API reference and corpus index.
The compiler suite passes, including all 362 library tests, measured tsc headers, JS corpus, and generated-docs checks.
The tier corpus sweep compares 299 entries without skips; the API-reference test passes.
`cargo fmt --check` and `git diff --check` pass. `tools/gate.sh` does not run. No commit occurs.

Round 5: the LIR snapshot adds only a309 (12,363 bytes); 1,884,343 -> 1,896,706 bytes; the snapshot test passes in debug and release.

## Verification review fix round landing gate

The orchestrator restated the §144 premise and C22 as a rule with
measured sources (a search miss, a finished generator's value), after a
review found the generator source beside the search miss.
`goldens-moved 1` is the LIR text snapshot: `a309` is a generator entry.

```text
gate full 406ea3520ae45ba5ff2c1a82f8a690adfc518143 dirty:12 debug 2183/0/3 release 2180/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Open, outside §144

A finished `Generator<Box>` read as `Box` crashes the dev tier
(`subscript: program terminated abnormally (dev-JIT child signal 11)`);
`node` gives a `TypeError`. Measured by the review with
`const b: Box = r.value; print(\`${b.value}\`)` after two `next()`
calls; `tsc` and `subscript check` accept it. C8 zero-initializes the
done value for every `T`. A good-faith program reaches it; it needs its
own section (a trap or a rejection at the read).
