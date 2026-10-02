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

## Phase Review fix round

The join now calls the same nullable predicate as a written `T | null` annotation.
Boundary-struct unit tests cover both branch orders and nullable operands.
Numeric and string literals take the other branch type as context before the join.
Binary operators and conditionals share the literal-context helper.
The raw-type allowlist cites the §143 parameter route and pins the revised join sites.

The fixture harness reaches boundary structs through `a124-contextual-conditional`.
That entry now passes inferred nullable blends to the host in both branch orders, with nullable operands.
Its golden does not change. Its native calls exclude it from the interpreter, as its existing header states.
The dev JIT and ship C AOT produce the same a124 golden bytes.

New entry a315 covers i64, f64, u8, and string-alias literals in both orders, plus negative parenthesized literals.
Node v24.18.0, the interpreter, dev JIT, and ship C AOT produce its golden bytes.
The three engines also reproduce the unchanged a314 golden.
New reject r305 reports S008 for `flag ? b : 300`, with `b: u8`.
The unit test also checks the reverse order and requires exactly one range diagnostic.
TypeScript 5.9.2 accepts a124, a315, and r305 under strict mode with the project prelude and mirror.

At contract pin `1771549a`, the revised a124 fails with three errors; a315 fails with ten errors.
Before this fix, the round-1 CLI rejects the revised a124 with two errors and a315 with ten errors.
The §143 matrix compares 38,910 cells before and after this fix. Zero verdicts change; the changed-cell list is empty.
All 14 matrix tests and all 11 JS corpus tests pass.
Generated documents were rebuilt through `generate-api-reference`; only the corpus index changes.
No existing golden changes.

The fresh Phase Review reports no CRITICAL or MAJOR findings.
Its one MINOR finding was the absent fix-round note; this section closes it.

The final compiler suite passes: 881 tests pass, zero fail, and one existing test is ignored.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.
The reviewer confirms zero open findings after this note. No files are staged or committed.
`tools/gate.sh` was not run, as the handoff requires.

## Phase Review witness fix round

The four contextual boundary constructor calls in a124 match HEAD exactly.
The four inferred locals remain additional forms, each with a separate printed host result.
The golden grows from 12 lines to 16 lines. Its diff adds four lines and changes no existing line.
A byte-prefix comparison also verifies that all original golden bytes remain unchanged.
The dev JIT and ship C AOT each produce the complete 16-line golden.

A direct interpreter run produces the first four reference lines, then stops at `subDeviceCreate`.
The measured error is `unsupported: subDeviceCreate requires a native library`.
The existing interpreter exclusion therefore still applies; three-engine output agreement cannot be measured for this native fixture entry.
The temporary interpreter probe was removed after the measurement.

`cargo test --offline --locked -p subscript-compiler` passes: 881 tests pass, zero fail, and one existing test is ignored.
The focused a124 two-tier golden test passes.
`cargo fmt --check` and `git diff --check` pass.
`tools/gate.sh` was not run. No files are staged or committed.

## Phase Review fix round landing gate

`goldens-moved 1` is `a124`: four added lines for the inferred boundary
forms; no existing line changed.

```text
gate full 164a960abe388d0d48b2bff49c7e670283d06905 dirty:12 debug 2194/0/3 release 2191/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Phase Review result

One review pass found MAJOR 1 (boundary structs excluded from the join:
the join restated the nullable predicate) and MINOR 2 (a literal branch
took no context; an allowlist reason cited the wrong section). All three
are fixed; rules 1b and 1b2 state the fixes. §146 is COMPLETE.
