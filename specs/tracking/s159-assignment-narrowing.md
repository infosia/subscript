# §159 Assignment narrowing

## Evidence at the contract pin

The pin is `e6fbbe6707e7e5118a807f769ea46778ae6beb12`.
The working tree was clean before this round.
`cargo build --offline --locked -p subscript-cli` passed before any source change.

The HEAD CLI checked all 11 table rows with explicit null checks before implementation.
Rows 1–7 and 9–11 passed. Row 8 gave S011 with the C17 block.
No row used a form outside the subset.

The HEAD CLI rejected the final `a332-assignment-narrowing.ts` with 27 diagnostics.
The measured diagnostics follow. Positions are `line:column` in that entry.

| Code | Position | Message |
|---|---|---|
| S011 | 13:64 | `A \| null` may be null here; narrow with a null check first |
| S011 | 13:81 | `A \| null` may be null here; narrow with a null check first |
| S005 | 13:109 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 14:68 | `A \| null` may be null here; narrow with a null check first |
| S011 | 14:85 | `A \| null` may be null here; narrow with a null check first |
| S005 | 14:113 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 15:77 | `A \| null` may be null here; narrow with a null check first |
| S011 | 15:94 | `A \| null` may be null here; narrow with a null check first |
| S005 | 15:122 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 16:96 | `A \| null` may be null here; narrow with a null check first |
| S011 | 16:113 | `A \| null` may be null here; narrow with a null check first |
| S005 | 16:141 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 17:75 | `A \| null` may be null here; narrow with a null check first |
| S011 | 17:94 | `A \| null` may be null here; narrow with a null check first |
| S005 | 17:139 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 18:53 | `A \| null` may be null here; narrow with a null check first |
| S011 | 18:70 | `A \| null` may be null here; narrow with a null check first |
| S005 | 18:111 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S011 | 19:101 | `A \| null` may be null here; narrow with a null check first |
| S011 | 19:118 | `A \| null` may be null here; narrow with a null check first |
| S005 | 19:146 | nominal types are not interchangeable: the argument expects `A`, got `A \| null` |
| S100 | 20:85 | type `((i32) => i32) \| null` is not callable |
| S100 | 20:124 | type `((i32) => i32) \| null` is not callable |
| S011 | 21:99 | `A \| null` may be null here; narrow with a null check first |
| S011 | 22:85 | `A \| null` may be null here; narrow with a null check first |
| S011 | 23:102 | `A \| null` may be null here; narrow with a null check first |
| S011 | 24:80 | `A \| null` may be null here; narrow with a null check first |

`tsc` 5.9.2 accepted `a332`. `node` produced its new golden.
`tsc` rejected `r363`, `r364`, and `r365` with TS18047.
The test headers state those measured results.

## Table rows

The unit test checks each row without engine execution.
Each row also carries its measured `tsc` result in the test source.

| Row | Form | tsc 5.9.2 | Checker after the change |
|---|---|---|---|
| 1 | Non-null let initializer | accepts | accepts |
| 2 | Non-null const initializer | accepts | accepts |
| 3 | Non-null assignment | accepts | accepts |
| 4 | Assignment in the null branch | accepts | accepts |
| 5 | Assignment in one opaque branch | TS18047 | S011, no divergence block |
| 6 | Null reassignment | TS18047 | S011, no divergence block |
| 7 | Field assignment | accepts | accepts |
| 8 | Field assignment, call, read | accepts | S011, C17 block |
| 9 | Module-global assignment | accepts | accepts |
| 10 | Assignment only in a for body | TS18047 | S011, no divergence block |
| 11 | Two non-null conditional arms | accepts | accepts |

The test adds controls for nullable writes, branch merges, terminal branches, null checks, aliases, and shadowed bindings.
The null branch of an already non-null local gives TS2339; the checker rejects that read with S011.
The test costs 0.03 seconds for 28 checker programs. It runs no engine or compiler subprocess.
The existing corpus tests execute `a332`; this round adds no duplicate tier test.

## Change

Initializers and assignments add facts to `FnCtx::narrowed`, the existing null-check state.
The separate `Scope::nonnull_flow` state and its snapshots leave the checker.
Branch and conditional merges intersect their outgoing facts.
Terminal branches contribute no edge to the merge.
The existing loop summaries keep the zero-iteration rule.

The scope form now carries outer facts that a declaration hides.
It carries their shared-location class and C17 end facts, so a call cannot restore a hidden shared fact.
This change supplies the binding fact that the previous name-based set did not carry.

Conditional expressions retain the common non-null type of their arms.
Simple assignment expressions retain the value type.
Null comparisons and nullable operators read the declared storage type of a narrowed location.
A nested nullish fallback retains its legal nullable receiver form.

C17 kills and the shared-read runtime conversion remain in force.
Branch merges retain C17 end facts from both paths.
Shared nullable arguments use `NullableNominalAssignmentShared` or `NullableAssignmentShared` with the existing C17 divergence.
The total witness table carries measured tsc-accepted witnesses for both sites.

The three `NonNullFlow` rejection sites and their divergence variants leave the tables.
Their accepted source witnesses remain available to the total checker test.
The generic matrix removes its obsolete C24 row 26 expectation.
Its omitted concrete instance count changes from 7,698 to 7,740 after that blanket exception leaves the control filter.
The generator updates the corpus index.
No existing `.expected` golden changes.

## Self-check

`cargo test --offline --locked -p subscript-compiler` passed: 999 tests, 97.02 seconds.
`cargo test --offline --locked -p subscript-codegen` passed: 726 tests, 227.08 seconds.
`cargo fmt --check` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed.
Its library warning counts are 5/18/13 for compiler/runtime/codegen, below or equal to the gate baseline 7/18/13.
Clippy reports no new warning against that baseline.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passed.
The full accept project passed `tsc` 5.9.2.
`git diff --check` passed.
Every changed Rust file stays below 2,001 lines.
`tools/gate.sh` did not run. No commit was created.

## Round 2 evidence

The round 1 working tree stays in place.
The existing CLI rejected the added assignment-check rows of `a332` with eight S011 diagnostics.
A separate assignment-result probe gave an internal LIR error: `nullable-to-value conversion requires NarrowNonNull: Coerce`.
The try/null-store probe gave S011 with the C17 block before this change.

The LIR assignment keeps the value operand separate from its storage conversion.
Each consumer receives the value operand with its own type, including a `null` result.
The embedded-header box conversion retains its projection and its local-origin result conversion.
The assignment does not read the target again.
Null comparisons of a simple assignment derive their path from the target in the existing narrowing state.
Shared targets retain the same C17 effects and shared-read runtime conversions.
The try handler uses the body's outgoing facts and C17 notes to distinguish a direct nullable write from an alias effect.
The fallback comment states the current contract.

`a332` adds inferred and annotated assignment bindings, template reads, function calls, field reads, an `if`, and both conditional arms.
It also adds assignment null checks in a `while`, an `&&`, an equality false branch, and a loose inequality.
The reversed null comparison and a shared-field check have controls in the same entry.
`node` v24.18.0 produced the new `a332` output with TypeScript 5.9.2.
The added output prefix follows; the original output follows it without a change.

```text
2
2
2
1
3
assignment-if
4
5
2
1
3
4
5
6
assignment-null
7
```

The checker test covers 40 programs in 0.04 seconds, with no engine or compiler subprocess.
A single TypeScript 5.9.2 program measured all 40 test sources.
It accepts each accepted control, including the C17 controls.
It gives TS18047 for the null branch, the read after the loop, and the try/null-store read.
The older impossible null branch gives TS2339.

## Open contract items

§159.4 stays open. This round makes no change for its three items.
The non-null alias/null comparison retains S100 with the C7 block; TypeScript 5.9.2 accepts the probe.
The loop-head reassignment and the captured const-local probes each give S011; TypeScript 5.9.2 accepts both.

## Round 2 self-check

`cargo test --offline --locked -p subscript-compiler` passed: 999 tests.
`cargo test --offline --locked -p subscript-codegen` passed: 726 tests.
The standing corpus tests cover the added `a332` rows on the interpreter, dev JIT, and ship C.
Their outputs match the `node` output byte for byte.
`cargo fmt --check` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed.
Its library warning counts are 5/18/13 for compiler/runtime/codegen, within the gate baseline 7/18/13.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passed.
The full accept project passed TypeScript 5.9.2.
`bash tools/hygiene.sh` passed.
`git diff --check` passed.
Every changed Rust file stays below 2,001 lines.
`tools/gate.sh` did not run. No commit was created.


## Round 3 evidence

The round 1–2 working tree stays in place.
The earlier CLI gave S100 with `NonNullableNullEquality` for five non-null assignment comparisons and the generic-field comparison.
It gave S011 with C17 for each of the four nullable-store probes.
The new checker tests failed before the source changes.

A null comparison of a plain assignment now reads the target's declared storage type.
The assignment retains its value operand and does not read the target again.
The effect form now carries the paths of nullable stores.
Each summary preserves those paths across a loop back edge and a `try` body, including a later non-null store.
A nullable store through the same path removes the C17 note for that path and its extensions.
A store through a shadowed binding remains a store through another receiver.
The effect summary excludes body-local receiver paths from the same-path nullable-store set at each scope boundary.
The summary retains their field effects, so they still end outer shared facts through C17.
Loop and try controls use the same receiver spelling in an inner scope and retain C17.
The handler uses the shared effect form; the outgoing-body heuristic leaves the checker.
The try join excludes C17 notes when a nullable store can precede the handler edge.
A control restores the value and calls other code again; its final read stays S011 without C17.
TypeScript 5.9.2 gives TS18047 for that control.
Terminal handlers and handlers that restore the value keep the body's C17 call note; TypeScript accepts those controls.

The checker test covers 56 programs in 0.09 seconds.
A separate generic-body test costs 0.01 seconds for one program.
Both tests use no engine or compiler subprocess.
The controls keep C17 for another receiver, a call in a condition, a static field after a call, and a call in a try body.
TypeScript 5.9.2 accepted the six new assignment-comparison shapes.
It rejected all four nullable-store shapes with TS18047.
A single TypeScript program also measured the existing table rows and controls.
The measured labels match the test comments.

`a332` adds the six accepted shapes to the existing tier tests.
Node v24.18.0 produced this new output prefix:

```text
8
9
true
true
4
10
```

The round 2 output follows without a change.
No other golden changes.

## Additional missing facts

These probes stay outside this change, as the handoff requires.
TypeScript 5.9.2 accepts each probe. The earlier CLI gives S011 without a divergence block for each read.

| Form | Missing fact |
|---|---|
| `let a: A \| null = null; while (true) { a = new A(); break; } a.x;` | The loop exit does not retain the assignment fact on the break edge. |
| `let a: A \| null = null; if (!((a = mk()) === null)) { a.x; }` | The negated assignment comparison does not supply the non-null branch fact. |
| `const items: (A \| null)[] = [null]; items[0] = new A(); items[0].x;` | An indexed target has no assignment narrowing path. |


## Round 3 self-check

`cargo test --offline --locked -p subscript-compiler` passed: 1,000 tests.
`cargo test --offline --locked -p subscript-codegen` passed: 726 tests.
The existing tier tests match the extended `a332` output against its Node golden.
`cargo fmt --check` passed.
`cargo clippy --offline --locked --workspace --all-targets` passed.
Its library warning counts are 5/18/13 for compiler/runtime/codegen, within the gate baseline 7/18/13.
Clippy reports no new warning against that baseline.
`cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` passed.
The full accept project passed TypeScript 5.9.2.
A single TypeScript program measured all 57 checker programs and the three additional missing-fact probes.
Each measured label matches its test comment or tracking row.
`bash tools/hygiene.sh` passed.
`git diff --check` passed.
Every changed Rust file stays below 2,001 lines; the largest has 1,730 lines.
`tools/gate.sh` did not run. No commit was created.

## Phase Review

| Pass | CRITICAL | MAJOR | MINOR | Result |
|---|---:|---:|---:|---|
| 1 | 0 | 2 | 6 | Fixed in round 2: an assignment used as a value has the value's type on every tier (rule 1); a null check of an assignment narrows its target (rule 1a). The 42 newly omitted generic-matrix cells are tsc-rejected controls (TS2322, TS2345), so their omission is correct. |
| 2 | 0 | 2 | 4 | Fixed in round 3: the null check of an assignment compares the target's declared type (rule 1b); a nullable store through the same path ends narrowing as rule 1 in every flow construct (rule 5). |

After round 3 the pass 2 probes are accepted (rule 1b) or rejected with
no block (rule 5), as `tsc` gives.

Final gate: `gate full fb0aa73c dirty:33 debug 2323/0/3 release 2320/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

Status: COMPLETE.
