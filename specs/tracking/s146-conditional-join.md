# §146 round 1 — conditional branch joins

The checker joins equal types and legal nullable pairs without a contextual type.
The join is symmetric. Contextual typing and generic union typing keep their existing paths.
A pair without a join reports both branch types and the C7 or C3 divergence.

## Old-rule search

The search covered the corpus, examples, docs, generated docs, and compiler tests.
These files pinned the old rejection:

- `corpus/reject/r119-conditional-without-context.ts` (deleted, unstaged).
- `compiler/tests/corpus_reject.rs` (the r119 row).
- `compiler/src/tests/language.rs` (the directional-rule unit test).
- `generated-docs/corpus-index.md` (the generated r119 row).

The old diagnostic explanation also appeared in `compiler/src/divergence.rs`; its unused topic was removed.
No file outside the handoff file set needed a change.
The orchestrator owns the `retired:r119-conditional-without-context` record.

## Red and external measurements

The CLI was built offline from the contract pin `1771549a` in a separate archive checkout.
`subscript check corpus/accept/a314-conditional-join.ts` exited 1 with 15 errors.
The principal output was:

```text
18: S100: type mismatch: the else branch expects `Box`, got `null`
19: S100: operator `!==` not defined for `Box` and `null`
20: S100: type mismatch: the else branch expects `null`, got `Box`
20: S100: cannot infer a type from `null`; annotate the declaration
22: S005: nominal types are not interchangeable: the else branch expects `Box`, got `Box | null`
23: S100: operator `!==` not defined for `Box` and `null`
26: S100: type mismatch: the else branch expects `() => i32`, got `null`
27: S100: operator `!==` not defined for `() => i32` and `null`
28: S100: type mismatch: the else branch expects `null`, got `() => i32`
28: S100: cannot infer a type from `null`; annotate the declaration
30: S100: type mismatch: the else branch expects `Box`, got `null` (twice)
31: S100: operator `!==` not defined for `Box` and `null`
32: S100: type mismatch: the else branch expects `null`, got `Box`
32: S100: cannot infer a type from `null`; annotate the declaration
error: 15 error(s)
```

TypeScript 5.9.2 accepted a314 and r302–r304 with strict mode and the project prelude.
Node v24.18.0 produced the exact a314 golden bytes.
The fixture handle needs the native fixture harness, so a314 covers references and functions.
The golden records both branch orders, nullable operands, nested conditionals, selected-branch calls, and result narrowing.
No existing `.expected` file changed.

## §143 matrix

The main matrix had 38,910 cells before and after the change.
The per-cell verdict comparison found zero changes. The list of changed verdicts is empty.
The compound-operator matrix compared 396 cells against the contract pin; zero verdicts changed.
`derived-union-shared-member-instance` still rejects two different classes.
Its diagnostic changed from S005/C1 to S100/C7; the matrix record now states the general-union restriction.
The optional `SUBSCRIPT_MATRIX_VERDICTS` output records each cell verdict for this comparison.

## Verification

The a314 golden matched the interpreter, dev JIT, and ship C AOT.
The JS corpus suite passed all 11 tests.
The generators rebuilt `generated-docs/`; only the corpus index changed.

The focused checker test covers opaque handles, null, and nullable handle operands in both orders.
The full compiler test suite and all 14 generic matrix tests passed.
`cargo fmt --check` and `git diff --check` passed.

The fresh review reported zero CRITICAL, MAJOR, or MINOR findings after the fixes.
`tools/hygiene.sh` passed. No files were staged or committed.

## Landing gate

```text
gate full 3ad2a10f24d0460dcdc9f6a53372a21b71e2308d dirty:15 debug 2191/0/3 release 2188/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
