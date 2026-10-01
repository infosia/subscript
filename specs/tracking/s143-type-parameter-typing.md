# §143 — Type parameter typing

Contract: `specs/blocks/compiler/s143-a-type-parameter-is-typed-as-tsc-types-it.md`.
Contract pin: `a9230193d22bebd979f635aaba67afc223bb9fa0`.
Red checker pin: `1b31a9a91df87a9c9173794bb37573faa57ea1f8`.

## Earlier stops

Round 1 stopped on the file set at the type-form compile probe (six E0004, two outside the list).
Round 2 stopped because `field_values.rs::a_generic_class_is_checked_per_instance` required two diagnostics, but §143 requires one.

## Round 3 collision stop

The matrix needs a collision id for loose equality and loose inequality.
The checker rejects both operators, regardless of the type argument.
Stock `tsc` 5.9.2 accepts both operators on each of the five required parameter kinds.
`specs/blocks/collisions.md` has no record of this operator restriction.
Q25 records the coercing APIs `Number(x)`, `isNaN`, and `isFinite`, not a ban on loose equality.
The generic-body gap in §3 records missing diagnostics, not this restriction.
No existing collision id records these ten cells.
The round 3 handoff requires a stop when a matrix cell needs a collision id that does not exist.

Each cell has no instance. The common declarations are:

```ts
class Box { v: i32 = 1; get(): i32 { return this.v; } }
function take<A extends Box>(a: A): void {}
```

Each function has this form. Replace `CONSTRAINT`, `VALUE`, and `BODY` with the row values.

```ts
function g<TCONSTRAINT, UCONSTRAINT>(x: VALUE, y: VALUE, u: U, s: string, b: boolean, box: Box, numbers: i32[], values: T[]): void { BODY }
export function main(): void {}
```

The equality body is `const a = x == y;`.
The inequality body is `const a = x != y;`.

| Cell | Constraint | Value type | `tsc` | Checker |
|---|---|---|---|---|
| `plain-loose-equality` | none | `T` | accepts | S100, 3:116 |
| `plain-loose-inequality` | none | `T` | accepts | S100, 3:116 |
| `class-loose-equality` | `extends Box` | `T` | accepts | S100, 3:140 |
| `class-loose-inequality` | `extends Box` | `T` | accepts | S100, 3:140 |
| `numeric-loose-equality` | `extends i32` | `T` | accepts | S100, 3:140 |
| `numeric-loose-inequality` | `extends i32` | `T` | accepts | S100, 3:140 |
| `array-loose-equality` | `extends i32[]` | `T` | accepts | S100, 3:144 |
| `array-loose-inequality` | `extends i32[]` | `T` | accepts | S100, 3:144 |
| `nullable-loose-equality` | `extends Box` | `T \| null` | accepts | S100, 3:154 |
| `nullable-loose-inequality` | `extends Box` | `T \| null` | accepts | S100, 3:154 |

Each cell gives exactly one diagnostic, with no divergence tag:

```text
loose equality is not in the language; use `===` / `!==`
```

The provisional matrix test ran `tsc` once over all cells in one temporary project.
Its options match `compiler/tests/tsc_corpus.rs`, with `prelude/lang.d.ts`.
The same test called `check_program` in process for each cell.
It used a table of forms and the five parameter kinds.
The loose-equality form comes from the §135 residual-gap record.

## Rule 5 measurement

The temporary harness used `codegen/tests/corpus/mod.rs::entry_ids` and `entry_sources` for accept and warn programs.
It copied `trap_ids` and `trap_sources` directly from `codegen/tests/support/trap_corpus.rs`.
It therefore included the `t72` ambient mirror through the trap loader's own function.
It discovered all example `.ts` programs recursively and excluded ambient `.d.ts` files as standalone programs.
It loaded each example through `entry_sources` and added the engine mirror where the example names engine APIs.
No source set was hand-listed.

| Programs | Count |
|---|---:|
| Accept | 294 |
| Warn | 5 |
| Trap | 71 |
| Examples | 17 |
| Total | 387 |
| Source files, including ambient files | 511 |

Both measurements used the same harness and a debug compiler.
Times cover the checker calls and exclude source discovery and Rust compilation.
Each time is one pass, not a median.

| Measurement | Result |
|---|---|
| Pin programs rejected | 0 |
| Prototype programs rejected | 0 |
| Pin checker time | 1.405280042 s |
| Prototype checker time | 1.447908750 s |
| Time change | +3.03% |
| First provisional matrix | 320 cells, 80 failures, 0.508107625 s |
| Matrix with loose equality and inequality | 330 cells, 90 failures, 0.520345541 s |
| Matrix failures with no collision record | The ten cells above |
| Release build | Not run; the collision stop precedes Red |

The first provisional matrix exposed incomplete typing at operator, result-type, collection, and statement sites.
Its failures are measurements, not an acceptance result.
The added ten cells expose a missing collision record, independent of those incomplete sites.
No corpus source or example triggered the rule 5 rejection stop.

## Prototype form

`Type::TypeParameter` carried a declaration identity, a source name, and an optional constraint.
Root instantiation stored the constraint in the parameter binding.
`apparent_type` resolved a parameter through its constraint.
Receiver, member, call, index, binary-operator, and iteration sites used that function.
`assignable` rejected a concrete value assigned to `T` and allowed `T` to flow through its constraint.
`GenericNumber` and `GenericUnion` carried provisional operation result types.
The diagnostic merge reported all opaque diagnostics and kept the §135 rule 3 deduplication.
The HIR match arms gave their error-type result and cited §143.

The provisional named rule 2a list contained:

- `TemplateInterpolation`: concrete runtime formatter types.
- `ValueField`: concrete value-class field layouts.
- `BooleanContext`: concrete boolean operands; only unary `!` used it before the stop.
- `SizedNumeric`: planned concrete numeric-width checks; no site used it before the stop.

This list and the typing were incomplete. All prototype changes were removed.

## Red, tests, and gate

No `r290`–`r293` entry was added. No Red CLI or `tsc` header measurement ran.
No existing test expectation changed. The required matrix acceptance and its failure-kind unit test remain incomplete.
No corpus source, header, generated index, collision record, or `.expected` golden changed.
The temporary matrix and measurement tests were removed with the production probes.

No gate ran in round 3. No production change remains, so the round 2 gate exception applies.
No round 3 gate verdict line exists.

## File sizes

Only this tracking note remains changed. The final changed-Rust-file size list is empty.
The removed probe files had these line counts:

| Rust file | Lines |
|---|---:|
| `compiler/src/check/class_shape.rs` | 973 |
| `compiler/src/check/expr/call.rs` | 1517 |
| `compiler/src/check/expr/literal.rs` | 533 |
| `compiler/src/check/expr/member.rs` | 576 |
| `compiler/src/check/expr/namespace.rs` | 1273 |
| `compiler/src/check/expr/operator.rs` | 1329 |
| `compiler/src/check/generics.rs` | 611 |
| `compiler/src/check/json.rs` | 1579 |
| `compiler/src/check/opaque.rs` | 470 |
| `compiler/src/check/stmt.rs` | 1514 |
| `compiler/src/check/type_rules.rs` | 233 |
| `compiler/src/check/tyres.rs` | 664 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1250 |
| `compiler/tests/generic_tsc_matrix.rs` | 144 |
| `compiler/tests/s143_measure.rs` | 125 |

No probe Rust file exceeded 2,000 lines.

## Round 4 collision stop

C20 resolves the ten loose equality and inequality cells from round 3.
The restored prototype used S100 and C20 for those cells.
The first run had 330 cells, 16 failures, and a cost of 0.522938 s.
The next run had 330 cells, one failure, and a cost of 0.520704417 s.
These counts describe an incomplete prototype, not acceptance.

The expanded matrix had 386 cells, 15 failures, and a cost of 0.551912875 s.
Five failures need a collision id that does not exist.
The remaining ten failures concern incomplete operation types and nullable-constraint narrowing.
No acceptance result exists.

The matrix seed names `if`, `while`, `do`, and `for` conditions on `T`.
The seed is the Forms table in `s135-opaque-generics.md` at lines 148 and 262.
The expanded matrix includes each of those conditions.
`do…while` fails independently of the type parameter.
§124 acceptance item 10 keeps this statement outside the language surface.
`collisions.md` has no record of this restriction.
Rule 2a cannot defer it: the restriction does not depend on the argument type.
The handoff requires a stop when a cell needs a collision id that does not exist.

The five cells share the declarations and parameter lists of the round 3 matrix above.
Each cell uses this body:

```ts
do { break; } while (x);
```

| Cell | Constraint | Value type | `tsc` | Checker |
|---|---|---|---|---|
| `plain-do-condition` | none | `T` | accepts | S100, 3:106 |
| `class-do-condition` | `extends Box` | `T` | accepts | S100, 3:130 |
| `numeric-do-condition` | `extends i32` | `T` | accepts | S100, 3:130 |
| `array-do-condition` | `extends i32[]` | `T` | accepts | S100, 3:134 |
| `nullable-do-condition` | `extends Box` | `T \| null` | accepts | S100, 3:144 |

Each cell reports one diagnostic, with no divergence tag:

```text
statement form outside the decided surface
```

A separate project with these five cells and a concrete control gives `tsc` 5.9.2 exit 0.
The options match `tsc_corpus.rs`, with the ambient prelude.
The concrete control is:

```ts
export function main(): void { do { break; } while (true); }
```

The pin CLI rejects this control with S100 at 1:32 and the same message.
This measurement confirms that the missing collision concerns the statement, rather than generic typing.

### Round 4 measurements

The same loader-based rule 5 harness ran twice on the prototype.
It ran 387 programs and 511 sources each time, with zero rejected programs.
The checker times were 1.434803250 s and 1.417821125 s.
The latter time is 0.89% above the round 3 pin time of 1.405280042 s.
Each time covers one pass of checker calls, without source discovery or Rust compilation.
The round 3 rule 5 numbers remain valid.
The release build did not run because the collision stop came first.

The Red CLI came from a `git archive` of `1b31a9a9` and a separate debug build.
Stock `tsc` ran once over the five Red programs, with the corpus options and ambient prelude.

| Probe | Pin CLI | `tsc` | Diagnostic line |
|---|---|---|---:|
| `r290-generic-relational-operand` | accepts | TS2365 | 10 |
| `r291-generic-constraint-to-parameter` | accepts | TS2322 | 10 |
| `r292-generic-uninitialized-field` | accepts | TS2564 | 10 |
| `r293-generic-member-write` | accepts | TS2339 | 9 |
| `r294-loose-equality` | S100 at 11:17 | accepts | 11 |

The prototype added apparent-type consumers and named instance restrictions.
The provisional list contained `TemplateInterpolation`, `ValueField`, `SizedNumeric`, `BooleanContext`, `AssociativeKey`, `SwitchKind`, and `CastKind`.
The restrictions and result types remain incomplete.
The matrix's failure-kind unit test passed, but the matrix did not pass.
The field-value test still required two reports where §143 requires one.
Four opaque-generic tests still pinned diagnostics that §143 changes.
The constrained-nullable regression still failed through its apparent type.
No existing test expectation changed.

All production probes, temporary tests, and Red sources were removed after the stop.
No corpus header, generated document, or golden remains changed.
The C20 edit supplied by the orchestrator remains intact.
Only this tracking note records the round 4 work.
No production change remains, so the round 2 gate exception applies.
No round 4 gate verdict line exists.

### Round 4 probe file sizes

The removed Rust probes had these sizes before formatting.
The final changed-Rust-file size list is empty.

| Rust file | Lines |
|---|---:|
| `compiler/src/check/class_shape.rs` | 973 |
| `compiler/src/check/expr/aggregate.rs` | 389 |
| `compiler/src/check/expr/assign.rs` | 621 |
| `compiler/src/check/expr/call.rs` | 1517 |
| `compiler/src/check/expr/literal.rs` | 533 |
| `compiler/src/check/expr/member.rs` | 576 |
| `compiler/src/check/expr/method.rs` | 1538 |
| `compiler/src/check/expr/namespace.rs` | 1273 |
| `compiler/src/check/expr/operator.rs` | 1343 |
| `compiler/src/check/generics.rs` | 611 |
| `compiler/src/check/json.rs` | 1579 |
| `compiler/src/check/opaque.rs` | 511 |
| `compiler/src/check/stmt.rs` | 1516 |
| `compiler/src/check/type_rules.rs` | 234 |
| `compiler/src/check/tyres.rs` | 664 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1250 |
| `compiler/tests/generic_tsc_matrix.rs` | 166 |
| `compiler/tests/s143_measure.rs` | 125 |

No probe Rust file exceeded 2,000 lines.

## Round 5 implementation

The working tree retains the implementation. No commit was made.
The supplied C20 record stays unchanged.

### Form and restrictions

`Type::TypeParameter` carries a declaration identity, a source name, and an optional constraint.
`apparent_type` resolves values through that constraint at member, call, operator, index, iteration, and narrowing sites.
Assignment compares parameter identities, then follows the source constraint.
A concrete constraint value does not assign to its parameter.
Null narrowing preserves the identity and removes null from the constraint.
`GenericNumber` and `GenericUnion` carry operation results inside the opaque check.
The opaque check reports every diagnostic. The independent-diagnostic list and its site marks are removed.
The existing §135 rule 3 merge removes repeated reports and keeps an instance report with a different code.
HIR arms return their error-type result and cite §143.

One named list holds the §143 rule 2a restrictions:

| Restriction | Concrete decision |
|---|---|
| `TemplateInterpolation` | Runtime formatter and string concatenation operand kind |
| `ValueField` | Value-class field layout |
| `SizedNumeric` | Numeric operand width, operation, and result assignment |
| `BooleanContext` | Boolean operand and condition kind |
| `AssociativeKey` | Hash and equality kind for a Map or Set key |
| `SwitchKind` | Dispatch kind |
| `CastKind` | Runtime conversion kind |
| `RelationalKind` | Relational operand kind |

The final matrix has 396 cells and no failing cell.
It uses the five required parameter kinds, 76 forms, and 16 extra regression forms.
It includes linked parameter constraints and numeric results used by another unary operation.
The matrix runs stock `tsc` once, then checks each cell in process.
Measured cost: 0.547502500 s, including the TypeScript process and temporary project.
The test states 0.548 s in its comment.
Loose equality cells name S100 and C20.
The five `do…while` cells name S100 and compiler.md §124.
The test checks that each named record exists.
The failure-kind unit test builds a TS2365 cell and a concrete value-field restriction cell.
It checks rule 4a and rule 4b reports.
No cell needs an additional record or owner decision.
The zero-failure result satisfies §143 rule 6's retirement condition.
No spec outside this tracking note was edited.

### Final measurements

The rule 5 harness uses the corpus loader functions and the copied trap loader functions, including the t72 mirror.
It discovers the examples recursively and uses the same source sets as the earlier measurements.
The temporary harness was removed after measurement and index generation.

| Measurement | Result |
|---|---|
| Accept / warn / trap / examples | 294 / 5 / 71 / 17 |
| Programs / sources | 387 / 511 |
| Rejected programs | 0 |
| Pin checker time, one debug pass | 1.405280042 s |
| Final checker time, one debug pass | 1.436261750 s |
| Time change | +2.20% |
| Final matrix | 396 cells, 0 failures, 0.547502500 s |
| First expanded prototype, retained from round 4 | 386 cells, 15 failures, 0.551912875 s |
| Final release CLI build | Passed, 18.55 s |
| Ship build and execution of a12 on a temporary copy | Passed; `42:2.5` matches the golden |

The four Red entries were rechecked with the CLI from the Red pin.
Each is accepted there. The final checker rejects each at its header line.
Stock `tsc` 5.9.2 was remeasured in one project with the corpus options and the ambient prelude.

| Entry | Red pin | Final checker | `tsc` |
|---|---|---|---|
| r290 | Accepts | S100, line 10 | TS2365, line 10 |
| r291 | Accepts | S100, line 10 | TS2322, line 10 |
| r292 | Accepts | S100, line 10 | TS2564, line 10 |
| r293 | Accepts | S018, line 9 | TS2339, line 9 |
| r294 | S100, line 11 | S100 with C20, line 11 | Accepts |

r294 records the existing concrete restriction; it is not a Red entry for a new rejection.
The new `LooseEquality` divergence topic has a direct unit test.
The new type variants have a direct test for identity, constraint, display, contained types, and conservative layout.
The corpus index was generated through `render_corpus_index`.
No corpus source outside r290–r294 changed. No header outside these entries changed. No `.expected` golden changed.

### Changed expectations

| Test | Old expectation | New expectation | Rule / measured TypeScript result |
|---|---|---|---|
| field_values::a_generic_class_is_checked_per_instance (renamed to a_generic_class_reports_each_site_once) | Two missing-field reports for two instances | One report at the template field | §143 rules 2 and 3; TS2564 |
| opaque_generics::each_kept_kind_is_reported_in_an_uninstantiated_body, arithmetic control | Accepts `x + 1` | One S100 | §143 rule 1; TS2365 |
| Same test, string initializer control | Accepts string from T | One S100 | §143 rule 1b; TS2322 |
| Same test, class field assignment control | Accepts i32 from T | One S100 | §143 rule 1b; TS2322 |
| Same test, nullable parameter control | Accepts an unnarrowed member read | One S011 | §143 rule 1a; TS18047 |
| Same test, constrained callable control | Accepts a call of class-constrained T | One S100 | §143 rule 1a; TS2349 |
| a_form_that_tsc_rejects_for_a_type_parameter_is_left_to_the_instance (renamed) | No report without an instance; concrete operator report with one | One opaque S100 in either case | §143 rules 1 and 2; TS2365 |
| Same test, unknown-name firing control | One S016 | S016 and S100 | §143 rule 2; TS2304 and TS2365 |
| a_language_restriction_in_an_uninstantiated_body_is_left_to_the_instance (renamed) | No report without an instance | One S100 for a concrete string value field | §143 rule 2; tsc accepts, C2 |
| Same test, unknown-name firing control | One S016 | S016 and S100 | §143 rule 2; TS2304 and C2 |
| a_shared_location_kill_is_kept_in_an_uninstantiated_body, parameter control | Accepts an unnarrowed T-or-null read | One S011 | §143 rule 1a; TS18047 |

The type-parameter control sources in the kept-kind table stay intact and have exact diagnostic assertions.
The accepted-form table retains all forms and each unknown-name firing control.
`each_dropped_form_is_accepted` is renamed to `each_tsc_valid_form_is_accepted`; its accepted verdicts stay unchanged.
The operator and value-field tests include accepted controls.

### Validation and stops

Targeted tests passed: 356 library tests, 39 reject tests, 25 field-value tests, and 18 opaque-generic tests.
The final matrix and its failure-kind unit test passed.
No scope, corpus rejection, or undecided-cell stop condition occurred.
`cargo fmt --check` passed.
The required quick gate failed. The handoff requires a stop after a gate failure, with no retry.
The implementation remains in the working tree. No production change was reverted.

Two tests failed:

- `codegen/src/interpreter/completion_tests.rs::async_completion_layout`: the measured size is 72 bytes; the assertion expects 56.
- `compiler/tests/growing_instance_chain.rs::every_supported_wrapper_records_an_expanding_edge`: two diagnostics replace the expected one.
  The extra diagnostic is S014 for `T` as a Set key, at 1:31.
  The expected S100 instance-growth diagnostic also reports, at 1:25.

The matrix tests passed inside the gate. No corpus source or example rejection was recorded.
The gate reports 2 skips and no moved golden.
Gate record: `target/gate/20261001T135150Z-quick.md`.

```text
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:36 debug 2134/2/3 skips 2 goldens-moved 0 exit 1
```

The gate failure is the only stop condition of round 5.
Acceptance is incomplete because the required gate did not pass.

### Final changed Rust file sizes

| Rust file | Lines |
|---|---:|
| `compiler/src/check/class_shape.rs` | 975 |
| `compiler/src/check/expr/aggregate.rs` | 389 |
| `compiler/src/check/expr/array_of_and_map_copy.rs` | 127 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1515 |
| `compiler/src/check/expr/literal.rs` | 535 |
| `compiler/src/check/expr/member.rs` | 576 |
| `compiler/src/check/expr/method.rs` | 1542 |
| `compiler/src/check/expr/namespace.rs` | 1273 |
| `compiler/src/check/expr/operator.rs` | 1454 |
| `compiler/src/check/generics.rs` | 611 |
| `compiler/src/check/instance_chain.rs` | 269 |
| `compiler/src/check/json.rs` | 1579 |
| `compiler/src/check/mod.rs` | 1389 |
| `compiler/src/check/narrowing.rs` | 333 |
| `compiler/src/check/opaque.rs` | 548 |
| `compiler/src/check/pipeline.rs` | 410 |
| `compiler/src/check/stmt.rs` | 1528 |
| `compiler/src/check/type_rules.rs` | 242 |
| `compiler/src/check/tyres.rs` | 665 |
| `compiler/src/divergence.rs` | 1307 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1287 |
| `compiler/tests/corpus_reject.rs` | 1318 |
| `compiler/tests/field_values.rs` | 646 |
| `compiler/tests/generic_tsc_matrix.rs` | 518 |
| `compiler/tests/opaque_generics.rs` | 714 |

No changed Rust file exceeds 2,000 lines.

## Round 6 gate repairs

The working tree retains all earlier changes. No commit was made.
No codegen test, corpus source, corpus header, or golden changed in this round.

### Type layout

The pin measurement uses the compiler library built from `1b31a9a9`.
A separate Rust executable measures `size_of::<Type>()` against that library and the repaired library.

| Form | Bytes |
|---|---:|
| Pin `Type` | 24 |
| Inline parameter payload, before the repair | 40 |
| Repaired `Type` | 24 |
| `Vec<Type>` payload | 24 |
| Boxed parameter payload | 8 |
| Boxed union slice payload | 16 |

`TypeParameter(Box<TypeParameterType>)` stores the identity, source name, and constraint in a documented payload.
`GenericUnion(Box<[Type]>)` stores the union members outside the enum.
An inline `Vec<Type>` would keep `Type` above the pin size after the parameter repair.
The unit test pins `Type` to 24 bytes and checks both boxed payload sizes.
Its comment states the interpreter value layout dependency.
The parameter test directly checks the payload fields, display, identity, and conservative layout.

### Restriction audit

`AssociativeKey` now covers annotations, constructors, Map copies, and `Map.groupBy`.
The matrix adds `Set<T>` and `Map<T, i32>` annotations for all five parameter kinds.
The audit also adds container values, array annotations, nullable arguments, callback results, accumulators, partial accessors, and large fixed arrays.

The complete rule 2a list is:

| Restriction | Concrete decision |
|---|---|
| `TemplateInterpolation` | Formatter and string concatenation operand kind |
| `ValueField` | Value-class field layout |
| `SizedNumeric` | Numeric width, operation, and result assignment |
| `BooleanContext` | Boolean operand and condition kind |
| `AssociativeKey` | Map and Set key hash and equality kind |
| `ArrayElementKind` | Array callback input, result, accumulator, and equality-search kind |
| `PartialValueLayout` | Map get and array find nullable-pointer value layout |
| `ContainerArgument` | Array element, Map key/value, and Set element Context affinity |
| `NullableShape` | Reference shape of a nullable argument |
| `AggregateLayout` | FixedArray byte layout |
| `SwitchKind` | Dispatch kind |
| `CastKind` | Runtime conversion kind |
| `RelationalKind` | Relational operand kind |

The new list entries route these restrictions through `instance_restriction`.
Concrete checks retain their existing verdicts.
The matrix has 516 cells: 100 forms across five parameter kinds and 16 extra regressions.
The measured matrix cost is 0.692536625 seconds, with zero failures.
The test states 0.693 seconds and runs TypeScript once.
No cell needs a new restriction record or an owner decision.

A direct instance test checks six restrictions with opaque and concrete controls.
They cover Set keys, Map keys, array callback kinds, Map get values, nullable shapes, and FixedArray layouts.
The instance-chain tests pass without expectation changes.

### Validation

The type-size unit test passes.
The matrix, instance-chain, and opaque-generic test suites pass.
The required `cargo fmt --check` and quick gate pass after these repairs.

### Round 6 changed Rust file sizes

| Rust file | Lines |
|---|---:|
| `compiler/src/check/class_shape.rs` | 975 |
| `compiler/src/check/container_argument.rs` | 92 |
| `compiler/src/check/expr/aggregate.rs` | 389 |
| `compiler/src/check/expr/array_of_and_map_copy.rs` | 132 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1515 |
| `compiler/src/check/expr/literal.rs` | 535 |
| `compiler/src/check/expr/member.rs` | 576 |
| `compiler/src/check/expr/method.rs` | 1567 |
| `compiler/src/check/expr/namespace.rs` | 1273 |
| `compiler/src/check/expr/operator.rs` | 1454 |
| `compiler/src/check/generics.rs` | 611 |
| `compiler/src/check/instance_chain.rs` | 269 |
| `compiler/src/check/json.rs` | 1579 |
| `compiler/src/check/mod.rs` | 1389 |
| `compiler/src/check/narrowing.rs` | 333 |
| `compiler/src/check/opaque.rs` | 540 |
| `compiler/src/check/pipeline.rs` | 410 |
| `compiler/src/check/stmt.rs` | 1528 |
| `compiler/src/check/type_rules.rs` | 240 |
| `compiler/src/check/tyres.rs` | 678 |
| `compiler/src/divergence.rs` | 1307 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1307 |
| `compiler/tests/corpus_reject.rs` | 1318 |
| `compiler/tests/field_values.rs` | 646 |
| `compiler/tests/generic_tsc_matrix.rs` | 614 |
| `compiler/tests/opaque_generics.rs` | 760 |

No changed Rust file exceeds 2,000 lines.

### Round 6 gate result

The quick gate reports 2,138 passed tests, zero failed tests, three ignored tests, and two skips.
Both former gate failures pass without test changes.
The matrix passes inside the gate. No golden moves.
No scope stop, corpus rejection stop, undecided-cell stop, or gate failure occurs in this round.
Gate record: `target/gate/20261001T141102Z-quick.md`.

```text
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2138/0/3 skips 2 goldens-moved 0 exit 0
```

## Round 7 matrix repairs

Only `compiler/tests/generic_tsc_matrix.rs` and this note changed in this round.
The accepted checker form and rule 2a list stay intact. No commit was made.

### Columns and measurements

Each table form has a no-instance cell and an instance cell where a concrete argument accepts the form.
The default arguments are `i32`, `Box`, `i32`, `i32[]`, and `Box`, in kind order.
The nullable kind uses a `Box` value with a `T | null` parameter.
The plain kind uses another argument when the default fails the concrete check.
The table lists each omitted instance cell below. Its no-instance cell remains in the matrix.
The 17 extra regressions have 12 instance cells. Five lack an accepted concrete body.
The exact measured problem shape uses `function gf<T>(x: T): boolean { return x > 1; }` and `gf<i32>(3)`.

| Kind | No instance | One instance | Omitted instance cells |
|---|---:|---:|---:|
| plain | 100 | 91 | 9 |
| class | 100 | 39 | 61 |
| numeric | 100 | 57 | 43 |
| array | 100 | 22 | 78 |
| nullable | 100 | 26 | 74 |
| Extra regressions | 17 | 12 | 5 |
| Total | 517 | 247 | 270 |

The final matrix has 764 cells and zero failures. Its measured cost is 1.131841500 seconds.
This cost includes the temporary project, one TypeScript process, matrix checks, record reads, and concrete admission controls.
The test comment states 1.132 seconds.
Each emitted instance also checks a concrete control with the same body and arguments.
These controls remove the type parameters. They expose concrete errors that the opaque diagnostic merge can hide.
A control accepts, or reports only the cell's named body restriction: C20, §124, or C2.
The matrix applies rules 4a and 4b to both columns.

The copy form now uses `copied` as its local name. Its former local `b` duplicated the boolean parameter name.
Both columns now measure copy and assignment, without an unrelated duplicate-name error.

### Alternative plain arguments

Each listed form uses the following accepted concrete argument instead of `i32`.

| Type argument | Forms |
|---|---|
| `Box` | `member-read`, `member-write`, `method`, `array-nullable-element`, `map-nullable-value`, `array-find`, `map-get`, `constraint-to-parameter`, `parameter-to-constraint`, `nullable-declaration`, `nested-constraint` |
| `string` | `add-string`, `string-mismatch`, `iteration`, `spread`, `switch-string`, `string-compound` |
| `boolean` | `logical-or`, `logical-and`, `logical-or-mismatch`, `logical-and-mismatch`, `logical-right-mismatch`, `logical-identity`, `unary-bang`, `conditional`, `if-condition`, `logical-and-right`, `while-condition`, `for-condition` |
| `Box \| null` | `nullish-same`, `nullish-box`, `nullish-mismatch`, `narrowing` |
| `() => void` | `call` |

### Omitted instance cells

The probe checks each concrete body and signature before the matrix admits its instance.
For the plain kind, the probe tries `i32`, `boolean`, `string`, `Box`, and `Box | null`.
A separate callable probe accepts `() => void`; that instance remains in the matrix.
The other four kinds use arguments that meet their fixed constraints.
Numeric, array, and Box constraints cannot supply boolean or unrelated numeric values to resolve these failures.
Null values do not change the nullable parameter's written type or its container rules.
Each row names an omitted instance cell and quotes its first concrete diagnostic.
Other diagnostics from the same control do not supply an accepted argument.

The large-array probe also tries `i8`, whose element size is one byte.
Its concrete body exceeds the 2147483632-byte stack-frame limit.
An `unreachable()` argument also fails because the compiler permits that call only as a statement.
No table kind supplies an accepted large-array instance.

| Omitted cell | Concrete reason |
|---|---|
| `plain-relational-string-instance` | operator not defined for `i32` and `string` |
| `plain-relational-boolean-instance` | operator not defined for `i32` and `boolean` |
| `plain-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `plain-map-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `plain-set-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `plain-relational-class-instance` | operator not defined for `i32` and `Box` |
| `plain-logical-relational-instance` | logical operators require booleans, got `i32` |
| `plain-nullish-relational-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `plain-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32` |
| `class-call-instance` | type `Box` is not callable |
| `class-add-literal-instance` | operator not defined for `Box` and `i32` |
| `class-add-same-instance` | operator not defined for `Box` and `Box` |
| `class-add-string-instance` | operator not defined for `Box` and `string` |
| `class-subtract-instance` | operator not defined for `Box` and `Box` |
| `class-multiply-instance` | operator not defined for `Box` and `i32` |
| `class-divide-instance` | operator not defined for `Box` and `Box` |
| `class-remainder-instance` | operator not defined for `Box` and `Box` |
| `class-bitwise-instance` | operator not defined for `Box` and `Box` |
| `class-shift-instance` | operator not defined for `Box` and `Box` |
| `class-relational-literal-instance` | operator not defined for `Box` and `i32` |
| `class-relational-same-instance` | operator not defined for `Box` and `Box` |
| `class-relational-distinct-instance` | operator not defined for `Box` and `Box` |
| `class-relational-string-instance` | operator not defined for `Box` and `string` |
| `class-relational-boolean-instance` | operator not defined for `Box` and `boolean` |
| `class-equality-number-instance` | operator not defined for `Box` and `i32` |
| `class-assert-number-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32` |
| `class-assert-distinct-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `Box` |
| `class-logical-or-instance` | logical operators require booleans, got `Box` |
| `class-logical-and-instance` | logical operators require booleans, got `Box` |
| `class-logical-or-mismatch-instance` | logical operators require booleans, got `Box` |
| `class-logical-and-mismatch-instance` | logical operators require booleans, got `Box` |
| `class-logical-right-mismatch-instance` | logical operators require booleans, got `Box` |
| `class-logical-identity-instance` | logical operators require booleans, got `Box` |
| `class-unary-minus-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-unary-tilde-instance` | `~` requires an integer operand, got `Box` |
| `class-unary-bang-instance` | `!` requires a boolean operand, got `Box` |
| `class-unary-mismatch-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-fresh-number-tilde-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-fresh-number-pair-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-fresh-number-literal-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-fresh-number-compare-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-fresh-number-array-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-template-instance` | type `Box` cannot be interpolated into a template |
| `class-array-join-instance` | `join` formats elements by the Q14 interpolation rules; `Box` elements are not interpolatable (Q22) |
| `class-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `class-map-nullable-key-instance` | `Box \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `class-set-nullable-key-instance` | `Box \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `class-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `Box` |
| `class-nullish-same-instance` | the left operand of `??` has type `Box`, which is not nullable |
| `class-nullish-box-instance` | the left operand of `??` has type `Box`, which is not nullable |
| `class-nullish-mismatch-instance` | the left operand of `??` has type `Box`, which is not nullable |
| `class-narrowing-instance` | operator not defined for `Box` and `null` |
| `class-iteration-instance` | `for…of` cannot make user class `Box` iterable (invariant 5): that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal; stock `tsc` rejects this subject too |
| `class-spread-instance` | array-literal spread accepts T[], FixedArray<T, N>, Set, or string; got `Box` |
| `class-index-instance` | array indices are `i32`, got `Box` |
| `class-update-instance` | `++`/`--` require a numeric target, got `Box` |
| `class-switch-same-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box` |
| `class-switch-number-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box` |
| `class-switch-string-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box` |
| `class-conditional-instance` | condition must be boolean, got `Box` |
| `class-if-condition-instance` | condition must be boolean, got `Box` |
| `class-string-compound-instance` | compound assignment is not defined for `string` |
| `class-logical-and-right-instance` | logical operators require booleans, got `Box` |
| `class-relational-class-instance` | operator not defined for `Box` and `Box` |
| `class-relational-fresh-instance` | unary `-` requires a numeric operand, got `Box` |
| `class-logical-relational-instance` | logical operators require booleans, got `Box` |
| `class-nullish-relational-instance` | the left operand of `??` has type `Box`, which is not nullable |
| `class-while-condition-instance` | condition must be boolean, got `Box` |
| `class-for-condition-instance` | condition must be boolean, got `Box` |
| `class-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `Box` |
| `numeric-member-read-instance` | `i32` has no member `v` |
| `numeric-member-write-instance` | `i32` has no member `v` |
| `numeric-method-instance` | `i32` has no method `get` |
| `numeric-call-instance` | type `i32` is not callable |
| `numeric-add-string-instance` | operator not defined for `i32` and `string` |
| `numeric-relational-string-instance` | operator not defined for `i32` and `string` |
| `numeric-relational-boolean-instance` | operator not defined for `i32` and `boolean` |
| `numeric-logical-or-instance` | logical operators require booleans, got `i32` |
| `numeric-logical-and-instance` | logical operators require booleans, got `i32` |
| `numeric-logical-or-mismatch-instance` | logical operators require booleans, got `i32` |
| `numeric-logical-and-mismatch-instance` | logical operators require booleans, got `i32` |
| `numeric-logical-right-mismatch-instance` | logical operators require booleans, got `i32` |
| `numeric-logical-identity-instance` | logical operators require booleans, got `i32` |
| `numeric-unary-bang-instance` | `!` requires a boolean operand, got `i32` |
| `numeric-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `numeric-array-nullable-element-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `numeric-map-nullable-value-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `numeric-map-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `numeric-set-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `numeric-array-find-instance` | `find` is rejected: A scalar element type has no miss value; use `findIndex` (Q22) |
| `numeric-map-get-instance` | `get(key)` is rejected: A scalar value type has no null miss value; use `getOr` (Q24) |
| `numeric-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `i32` |
| `numeric-constraint-to-parameter-instance` | type mismatch: the initializer expects `i32`, got `Box` |
| `numeric-parameter-to-constraint-instance` | type mismatch: the initializer expects `Box`, got `i32` |
| `numeric-nullable-declaration-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `numeric-nullish-same-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `numeric-nullish-box-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `numeric-nullish-mismatch-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `numeric-narrowing-instance` | operator not defined for `i32` and `null` |
| `numeric-iteration-instance` | `for…of` accepts only T[], FixedArray<T, N>, Set, string, or Generator<T>; got `i32` |
| `numeric-spread-instance` | array-literal spread accepts T[], FixedArray<T, N>, Set, or string; got `i32` |
| `numeric-switch-string-instance` | type mismatch: the case label expects `i32`, got `string` |
| `numeric-conditional-instance` | condition must be boolean, got `i32` |
| `numeric-if-condition-instance` | condition must be boolean, got `i32` |
| `numeric-string-compound-instance` | compound assignment is not defined for `string` |
| `numeric-logical-and-right-instance` | logical operators require booleans, got `i32` |
| `numeric-relational-class-instance` | operator not defined for `i32` and `Box` |
| `numeric-logical-relational-instance` | logical operators require booleans, got `i32` |
| `numeric-nullish-relational-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `numeric-while-condition-instance` | condition must be boolean, got `i32` |
| `numeric-for-condition-instance` | condition must be boolean, got `i32` |
| `numeric-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32` |
| `numeric-nested-constraint-instance` | type argument `i32` does not satisfy the constraint `Box` of `A` |
| `array-member-read-instance` | `v` is outside the array surface (length, indexing, push, pop, and the Q22 Array methods) |
| `array-member-write-instance` | `v` is outside the array surface (length, indexing, push, pop, and the Q22 Array methods) |
| `array-method-instance` | `get` is outside the array surface (length, indexing, push, pop, and the Q22 Array methods) |
| `array-call-instance` | type `i32[]` is not callable |
| `array-add-literal-instance` | operator not defined for `i32[]` and `i32` |
| `array-add-same-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-add-string-instance` | operator not defined for `i32[]` and `string` |
| `array-subtract-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-multiply-instance` | operator not defined for `i32[]` and `i32` |
| `array-divide-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-remainder-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-bitwise-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-shift-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-relational-literal-instance` | operator not defined for `i32[]` and `i32` |
| `array-relational-same-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-relational-distinct-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-relational-string-instance` | operator not defined for `i32[]` and `string` |
| `array-relational-boolean-instance` | operator not defined for `i32[]` and `boolean` |
| `array-equality-same-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-equality-distinct-instance` | operator not defined for `i32[]` and `i32[]` |
| `array-equality-number-instance` | operator not defined for `i32[]` and `i32` |
| `array-assert-number-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `i32[]` to `i32` |
| `array-assert-distinct-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `i32[]` to `i32[]` |
| `array-logical-or-instance` | logical operators require booleans, got `i32[]` |
| `array-logical-and-instance` | logical operators require booleans, got `i32[]` |
| `array-logical-or-mismatch-instance` | logical operators require booleans, got `i32[]` |
| `array-logical-and-mismatch-instance` | logical operators require booleans, got `i32[]` |
| `array-logical-right-mismatch-instance` | logical operators require booleans, got `i32[]` |
| `array-logical-identity-instance` | logical operators require booleans, got `i32[]` |
| `array-unary-minus-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-unary-tilde-instance` | `~` requires an integer operand, got `i32[]` |
| `array-unary-bang-instance` | `!` requires a boolean operand, got `i32[]` |
| `array-unary-mismatch-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-fresh-number-tilde-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-fresh-number-pair-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-fresh-number-literal-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-fresh-number-compare-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-fresh-number-array-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-template-instance` | type `i32[]` cannot be interpolated into a template |
| `array-array-join-instance` | `join` formats elements by the Q14 interpolation rules; `i32[]` elements are not interpolatable (Q22) |
| `array-set-annotation-instance` | `i32[]` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `array-map-key-annotation-instance` | `i32[]` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `array-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `array-map-copy-instance` | `i32[]` is not a permitted Map/Set key kind (Q24) |
| `array-map-group-key-instance` | `Map.groupBy` callback returns `i32[]`, which is not a §10.2 Map/Set key kind (Q24) |
| `array-map-set-instance` | `i32[]` is not a permitted Map/Set key kind (Q24) |
| `array-set-add-instance` | `i32[]` is not a permitted Map/Set key kind (Q24) |
| `array-array-nullable-element-instance` | unions are limited to `Ref \| null`; `i32[] \| null` is not a reference type union |
| `array-map-nullable-value-instance` | unions are limited to `Ref \| null`; `i32[] \| null` is not a reference type union |
| `array-map-nullable-key-instance` | `i32[] \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `array-set-nullable-key-instance` | `i32[] \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `array-map-key-instance` | `i32[]` is not a permitted Map/Set key kind (Q24) |
| `array-set-key-instance` | `i32[]` is not a permitted Map/Set key kind (Q24) |
| `array-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `i32[]` |
| `array-constraint-to-parameter-instance` | type mismatch: the initializer expects `i32[]`, got `Box` |
| `array-parameter-to-constraint-instance` | type mismatch: the initializer expects `Box`, got `i32[]` |
| `array-nullable-declaration-instance` | unions are limited to `Ref \| null`; `i32[] \| null` is not a reference type union |
| `array-nullish-same-instance` | the left operand of `??` has type `i32[]`, which is not nullable |
| `array-nullish-box-instance` | the left operand of `??` has type `i32[]`, which is not nullable |
| `array-nullish-mismatch-instance` | the left operand of `??` has type `i32[]`, which is not nullable |
| `array-narrowing-instance` | operator not defined for `i32[]` and `null` |
| `array-index-instance` | array indices are `i32`, got `i32[]` |
| `array-update-instance` | `++`/`--` require a numeric target, got `i32[]` |
| `array-switch-same-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `i32[]` |
| `array-switch-number-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `i32[]` |
| `array-switch-string-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `i32[]` |
| `array-conditional-instance` | condition must be boolean, got `i32[]` |
| `array-if-condition-instance` | condition must be boolean, got `i32[]` |
| `array-string-compound-instance` | compound assignment is not defined for `string` |
| `array-logical-and-right-instance` | logical operators require booleans, got `i32[]` |
| `array-relational-class-instance` | operator not defined for `i32[]` and `Box` |
| `array-relational-fresh-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `array-logical-relational-instance` | logical operators require booleans, got `i32[]` |
| `array-nullish-relational-instance` | the left operand of `??` has type `i32[]`, which is not nullable |
| `array-while-condition-instance` | condition must be boolean, got `i32[]` |
| `array-for-condition-instance` | condition must be boolean, got `i32[]` |
| `array-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32[]` |
| `array-nested-constraint-instance` | type argument `i32[]` does not satisfy the constraint `Box` of `A` |
| `nullable-identity-instance` | nominal types are not interchangeable: the initializer expects `Box`, got `Box \| null` |
| `nullable-member-read-instance` | `Box \| null` may be null here; narrow with a null check first |
| `nullable-member-write-instance` | `Box \| null` may be null here; narrow with a null check first |
| `nullable-method-instance` | `Box \| null` may be null here; narrow with a null check first |
| `nullable-call-instance` | type `Box \| null` is not callable |
| `nullable-add-literal-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-add-same-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-add-string-instance` | operator not defined for `Box \| null` and `string` |
| `nullable-subtract-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-multiply-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-divide-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-remainder-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-bitwise-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-shift-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-relational-literal-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-relational-same-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-relational-distinct-instance` | operator not defined for `Box \| null` and `Box` |
| `nullable-relational-string-instance` | operator not defined for `Box \| null` and `string` |
| `nullable-relational-boolean-instance` | operator not defined for `Box \| null` and `boolean` |
| `nullable-equality-same-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-equality-distinct-instance` | operator not defined for `Box \| null` and `Box` |
| `nullable-equality-number-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-assert-number-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box \| null` to `i32` |
| `nullable-assert-distinct-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box \| null` to `Box` |
| `nullable-logical-or-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-logical-and-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-logical-or-mismatch-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-logical-and-mismatch-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-logical-right-mismatch-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-logical-identity-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-unary-minus-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-unary-tilde-instance` | `~` requires an integer operand, got `Box \| null` |
| `nullable-unary-bang-instance` | `!` requires a boolean operand, got `Box \| null` |
| `nullable-unary-mismatch-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-fresh-number-tilde-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-fresh-number-pair-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-fresh-number-literal-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-fresh-number-compare-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-fresh-number-array-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-template-instance` | type `Box \| null` cannot be interpolated into a template |
| `nullable-array-join-instance` | `join` formats elements by the Q14 interpolation rules; `Box` elements are not interpolatable (Q22) |
| `nullable-array-push-instance` | nominal types are not interchangeable: the argument expects `Box`, got `Box \| null` |
| `nullable-fixed-array-annotation-instance` | nominal types are not interchangeable: the array element expects `Box`, got `Box \| null` |
| `nullable-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `nullable-map-set-instance` | nominal types are not interchangeable: the argument expects `Box`, got `Box \| null` |
| `nullable-set-add-instance` | nominal types are not interchangeable: the argument expects `Box`, got `Box \| null` |
| `nullable-array-of-instance` | nominal types are not interchangeable: the array element expects `Box`, got `Box \| null` |
| `nullable-map-nullable-key-instance` | `Box \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `nullable-set-nullable-key-instance` | `Box \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `nullable-array-map-output-instance` | nominal types are not interchangeable: the lambda body expects `Box`, got `Box \| null` |
| `nullable-array-reduce-instance` | nominal types are not interchangeable: the `reduce` init expects `Box`, got `Box \| null` |
| `nullable-array-reduce-right-instance` | nominal types are not interchangeable: the `reduceRight` init expects `Box`, got `Box \| null` |
| `nullable-array-includes-instance` | nominal types are not interchangeable: the argument expects `Box`, got `Box \| null` |
| `nullable-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `Box \| null` |
| `nullable-parameter-to-constraint-instance` | nominal types are not interchangeable: the initializer expects `Box`, got `Box \| null` |
| `nullable-iteration-instance` | `for…of` accepts only T[], FixedArray<T, N>, Set, string, or Generator<T>; got `Box \| null` |
| `nullable-spread-instance` | array-literal spread accepts T[], FixedArray<T, N>, Set, or string; got `Box \| null` |
| `nullable-index-instance` | array indices are `i32`, got `Box \| null` |
| `nullable-update-instance` | `++`/`--` require a numeric target, got `Box \| null` |
| `nullable-switch-same-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box \| null` |
| `nullable-switch-number-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box \| null` |
| `nullable-switch-string-instance` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `Box \| null` |
| `nullable-conditional-instance` | condition must be boolean, got `Box \| null` |
| `nullable-if-condition-instance` | condition must be boolean, got `Box \| null` |
| `nullable-string-compound-instance` | compound assignment is not defined for `string` |
| `nullable-logical-and-right-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-relational-class-instance` | operator not defined for `Box \| null` and `Box` |
| `nullable-relational-fresh-instance` | unary `-` requires a numeric operand, got `Box \| null` |
| `nullable-logical-relational-instance` | logical operators require booleans, got `Box \| null` |
| `nullable-nullish-relational-instance` | operator not defined for `Box \| null` and `Box \| null` |
| `nullable-while-condition-instance` | condition must be boolean, got `Box \| null` |
| `nullable-for-condition-instance` | condition must be boolean, got `Box \| null` |
| `nullable-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `Box` |
| `nullable-nested-constraint-instance` | nominal types are not interchangeable: the argument expects `Box`, got `Box \| null` |
| `nullable-constraint-minus-instance` | A Box or nullable Box is not a numeric unary operand. |
| `nullable-constraint-tilde-instance` | A Box or nullable Box is not an integer unary operand. |
| `uninitialized-field-instance` | No type argument initializes the concrete `v: i32` field. |
| `unknown-name-instance` | No type argument resolves the unknown function `nope`. |
| `const-write-instance` | No type argument permits assignment to the constant local `a`. |

### Restriction records

Each divergence carries a rule code, a record name, and a restriction token.
C20 states the rejection of `==` and `!=` with S100 on every operand type.
C2 states the field-type restriction for reference, string, and nullable fields in value classes.
Compiler §124 states that `do…while` stays outside the language surface.
The check reads each record from its heading to the next heading at that level or an enclosing level.
It fails rule 4b if that section lacks the token.
Compiler section links come from `specs/blocks/compiler.md` §0. No compiler section filename is hard-coded.

A unit test builds a string-field restriction cell that names C20 with the C2 field-type token.
The unchanged C20 record lacks that restriction. The cell reports rule 4b and the missing restriction.
Another test places a token in the next section and checks that it does not satisfy the first record.
The existing unit test still reports both rule 4a and rule 4b failures.
All four matrix-file tests pass.

### File sizes

| Rust file | Lines |
|---|---:|
| `compiler/src/check/class_shape.rs` | 975 |
| `compiler/src/check/container_argument.rs` | 92 |
| `compiler/src/check/expr/aggregate.rs` | 389 |
| `compiler/src/check/expr/array_of_and_map_copy.rs` | 132 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1515 |
| `compiler/src/check/expr/literal.rs` | 535 |
| `compiler/src/check/expr/member.rs` | 576 |
| `compiler/src/check/expr/method.rs` | 1567 |
| `compiler/src/check/expr/namespace.rs` | 1273 |
| `compiler/src/check/expr/operator.rs` | 1454 |
| `compiler/src/check/generics.rs` | 611 |
| `compiler/src/check/instance_chain.rs` | 269 |
| `compiler/src/check/json.rs` | 1579 |
| `compiler/src/check/mod.rs` | 1389 |
| `compiler/src/check/narrowing.rs` | 333 |
| `compiler/src/check/opaque.rs` | 540 |
| `compiler/src/check/pipeline.rs` | 410 |
| `compiler/src/check/stmt.rs` | 1528 |
| `compiler/src/check/type_rules.rs` | 240 |
| `compiler/src/check/tyres.rs` | 678 |
| `compiler/src/divergence.rs` | 1307 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1307 |
| `compiler/tests/corpus_reject.rs` | 1318 |
| `compiler/tests/field_values.rs` | 646 |
| `compiler/tests/generic_tsc_matrix.rs` | 1377 |
| `compiler/tests/opaque_generics.rs` | 760 |

No changed Rust file exceeds 2,000 lines. No line in the matrix file exceeds 100 characters.

### Round 7 validation

`cargo fmt --check` passes. The required quick gate passes.
The gate reports 2,140 passed tests, zero failed tests, three ignored tests, and two skips.
The 764-cell matrix and all three failure-check tests pass inside the gate.
No golden moves. No file-scope, corpus rejection, undecided-cell, or gate-failure stop occurs.
The working tree retains all earlier changes. No commit was made.
Gate record: `target/gate/20261001T143012Z-quick.md`.

```text
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2140/0/3 skips 2 goldens-moved 0 exit 0
```

## Landing gate

The orchestrator ran the full gate on the round 7 tree.

```text
gate full a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2140/0/3 release 2137/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

After the gate, rule 6 retired §135.3 and the generic-body item of
`collisions.md` §3. `generic_tsc_matrix`, `js_corpus`, and
`growing_instance_chain` passed again on that text.


## Phase Review fix round

| Finding | Fix | Test |
|---|---|---|
| MAJOR 1, MINOR 4: a deferral skips TypeScript operand typing | Each restriction tests the apparent type. Compound assignments check both operands through `bin_result`, then check the result against the destination. Accessors use that same result. | `compound_operands_and_callback_conditions_keep_typescript_typing`; matrix compound and callback cells |
| Concrete accessor compound assignment accepts a string result | A numeric getter plus a string produces `string`. The numeric setter rejects that result. | `an_accessor_compound_result_must_fit_the_setter`; reject entry `r295-accessor-compound-type` |
| MAJOR 2: a fresh `number` becomes `f64` | `apparent_type` preserves `GenericNumber`. Numeric operators admit it as a distinct operand kind. The operand-width rewrite is removed. Indices use the common assignment test. Updates accept `number`. | `fresh_number_has_no_sized_operand_kind`; matrix remainder, equality, index, and bitwise cells |
| MAJOR 3: a forward constraint keeps an incomplete copy | Apparent-type resolution looks up an unbound constraint by parameter identity. A cycle guard prevents repeated resolution. | `constraints_resolve_by_identity_in_both_declaration_orders`; both forward-constraint matrix columns |
| MINOR 5: missing matrix forms and numeric kinds | The table adds the requested forms, array iteration with compound assignment, and `extends f64` and `extends u8`. | 1,193 cells; 795 no-instance cells and 398 instance cells; zero failures |
| MINOR 6: a diagnostic prints `f64` | The number operand keeps its source-independent `number` type and diagnostic name. | The unary-result/string equality test asserts both `number` and `string`. |
| MINOR 7: concrete equality carries generic tags | The `r294` header uses `equality, numeric-operands`. The generator updates its index row. | Reject-header and corpus-index checks |
| MINOR 8: diagnostic Debug text breaks table cells | The omitted-instance table contains diagnostic messages only. Each pipe is escaped. | The concrete omission probe regenerates all table-form reasons. No Debug fields remain. |

### Restriction admission tests

Every test below requires a type that involves a parameter or a generic number result.
The apparent-type test applies before the project restriction defers.
Call arguments, callback signatures, assignment destinations, and cast overlap keep their separate type checks.

| Restriction | Apparent-type admission test | Project restriction |
|---|---|---|
| `TemplateInterpolation` | A resolved value type | Runtime formatter and concatenation kind |
| `ValueField` | A resolved field type | Value-class field layout |
| `SizedNumeric` | A numeric type or the distinct `number` result | Numeric width and sized result assignment |
| `UnaryNumeric` | A value other than null, nullable, or void | Unary coercion and integer representation |
| `BooleanContext` | A type other than a required function | Concrete boolean operand or condition |
| `AssociativeKey` | A resolved type argument | Concrete hash and equality kind |
| `ArrayElementKind` | A resolved element or callback result type | Runtime callback and equality-search representation |
| `PartialValueLayout` | A resolved result type | Nullable-pointer result representation |
| `ContainerArgument` | A resolved type argument | Context affinity |
| `NullableShape` | A resolved nullable base type | Nullable reference representation |
| `AggregateLayout` | A fixed-array type | Concrete aggregate byte layout |
| `SwitchKind` | A resolved discriminant type | Concrete dispatch kind |
| `CastKind` | A resolved type, with overlap checked at the cast | Runtime conversion kind |
| `RelationalKind` | A non-nullable value other than void or a function; operand overlap is required | Concrete relational operand kind |

The numeric kinds use an `i32` instance that satisfies all three numeric constraints under §135 rule 2b.
This keeps the instance admissible when a form needs an `i32` index, destination, or bitwise operand.
Each new form has a no-instance column for every kind.
Its instance column uses a concrete argument that the checker accepts.
The table lists omitted instance cells and their concrete diagnostics.
The callback condition and unary-number/string comparison have no accepted concrete instance.
The class, array, and nullable numeric forms have no accepted concrete operand.
Their no-instance cells remain in the matrix.

### Red and measurements

A CLI built from `36684546` accepts `r295-accessor-compound-type.ts` with no errors.
Stock TypeScript 5.9.2 rejects it with TS2322 at line 14.
The TypeScript options match `tsc_corpus.rs` and use the ambient prelude.
The current reject-table test requires S100 at that line and passes.

The rule 5 harness uses `entry_ids` and `entry_sources` from the codegen corpus loader.
It copies `trap_ids` and `trap_sources` from the trap loader, including the t72 mirrors.
It discovers every example recursively. The temporary harness is removed after the measurement.

| Measurement | Result |
|---|---|
| Accept / warn / trap / examples | 294 / 5 / 71 / 17 |
| Programs / source files | 387 / 511 |
| Rejected at `36684546` / current | 0 / 0 |
| Pin checker time, one debug pass | 1.470384000 s |
| Current checker time, one debug pass | 1.491782959 s |
| Checker time change | +1.46% |
| Expanded matrix cost, one TypeScript process | 1.623520166 s |
| Expanded matrix failures | 0 / 1,193 |

The new unit tests, the reject corpus, and all compiler integration tests pass.
No public API is added. No golden changes. No commit is made.

### Changed Rust file sizes

| Rust file | Lines |
|---|---:|
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/member.rs` | 575 |
| `compiler/src/check/expr/operator.rs` | 1469 |
| `compiler/src/check/opaque.rs` | 578 |
| `compiler/src/check/type_rules.rs` | 248 |
| `compiler/tests/corpus_reject.rs` | 1319 |
| `compiler/tests/generic_tsc_matrix.rs` | 1476 |
| `compiler/tests/opaque_generics.rs` | 854 |

Every changed Rust file stays below 2,000 lines.

The admission audit also measures void and null constraints with TypeScript 5.9.2.
TypeScript accepts interpolation, array join, and switch on a void-constrained parameter.
It also accepts a nullable declaration on a null-constrained parameter.
These slots admit every resolved surface type before the project representation check.
`unrestricted_typescript_slots_keep_void_and_null_constraints` checks these forms and a void-constrained value-class field.

The release CLI build passes in 20.10 seconds.
The hygiene check and `git diff --check` pass.
The first quick gate passes with 2,144 tests, zero failures, and three ignored tests.
It used the checker before the final void/null admission adjustment.
The final tree receives another quick gate after that adjustment.

### Added omitted instance cells

The no-instance column keeps each cell below. Its concrete instance fails with the stated diagnostic.

| Omitted cell | Concrete reason |
|---|---|
| `plain-callback-condition-instance` | condition must be boolean, got `(i32) => void` |
| `plain-minus-string-equality-instance` | operator not defined for `i32` and `string` |
| `class-iteration-compound-instance` | compound assignment is not defined for `i32` |
| `class-s-add-x-instance` | compound assignment is not defined for `i32` |
| `class-s-subtract-x-instance` | compound assignment is not defined for `i32` |
| `class-s-bitand-x-instance` | compound assignment is not defined for `i32` |
| `class-field-add-x-instance` | compound assignment is not defined for `i32` |
| `class-callback-condition-instance` | condition must be boolean, got `(Box) => void` |
| `class-remainder-equality-instance` | operator not defined for `Box` and `i32` |
| `class-fresh-equality-instance` | operator not defined for `Box` and `i32` |
| `class-fresh-index-instance` | operator not defined for `Box` and `i32` |
| `class-fresh-bitand-instance` | operator not defined for `Box` and `i32` |
| `class-minus-string-equality-instance` | unary `-` requires a numeric operand, got `Box` |
| `numeric-callback-condition-instance` | condition must be boolean, got `(i32) => void` |
| `numeric-minus-string-equality-instance` | operator not defined for `i32` and `string` |
| `f64-callback-condition-instance` | condition must be boolean, got `(i32) => void` |
| `f64-minus-string-equality-instance` | operator not defined for `i32` and `string` |
| `f64-member-read-instance` | `i32` has no member `v` |
| `f64-member-write-instance` | `i32` has no member `v` |
| `f64-method-instance` | `i32` has no method `get` |
| `f64-call-instance` | type `i32` is not callable |
| `f64-add-string-instance` | operator not defined for `i32` and `string` |
| `f64-relational-string-instance` | operator not defined for `i32` and `string` |
| `f64-relational-boolean-instance` | operator not defined for `i32` and `boolean` |
| `f64-logical-or-instance` | logical operators require booleans, got `i32` |
| `f64-logical-and-instance` | logical operators require booleans, got `i32` |
| `f64-logical-or-mismatch-instance` | logical operators require booleans, got `i32` |
| `f64-logical-and-mismatch-instance` | logical operators require booleans, got `i32` |
| `f64-logical-right-mismatch-instance` | logical operators require booleans, got `i32` |
| `f64-logical-identity-instance` | logical operators require booleans, got `i32` |
| `f64-unary-bang-instance` | `!` requires a boolean operand, got `i32` |
| `f64-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `f64-array-nullable-element-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `f64-map-nullable-value-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `f64-map-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `f64-set-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `f64-array-find-instance` | `find` is rejected: A scalar element type has no miss value; use `findIndex` (Q22) |
| `f64-map-get-instance` | `get(key)` is rejected: A scalar value type has no null miss value; use `getOr` (Q24) |
| `f64-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `i32` |
| `f64-constraint-to-parameter-instance` | type mismatch: the initializer expects `i32`, got `Box` |
| `f64-parameter-to-constraint-instance` | type mismatch: the initializer expects `Box`, got `i32` |
| `f64-nullable-declaration-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `f64-nullish-same-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `f64-nullish-box-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `f64-nullish-mismatch-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `f64-narrowing-instance` | operator not defined for `i32` and `null` |
| `f64-iteration-instance` | `for…of` accepts only T[], FixedArray<T, N>, Set, string, or Generator<T>; got `i32` |
| `f64-spread-instance` | array-literal spread accepts T[], FixedArray<T, N>, Set, or string; got `i32` |
| `f64-switch-string-instance` | type mismatch: the case label expects `i32`, got `string` |
| `f64-conditional-instance` | condition must be boolean, got `i32` |
| `f64-if-condition-instance` | condition must be boolean, got `i32` |
| `f64-string-compound-instance` | compound assignment is not defined for `string` |
| `f64-logical-and-right-instance` | logical operators require booleans, got `i32` |
| `f64-relational-class-instance` | operator not defined for `i32` and `Box` |
| `f64-logical-relational-instance` | logical operators require booleans, got `i32` |
| `f64-nullish-relational-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `f64-while-condition-instance` | condition must be boolean, got `i32` |
| `f64-for-condition-instance` | condition must be boolean, got `i32` |
| `f64-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32` |
| `f64-nested-constraint-instance` | type argument `i32` does not satisfy the constraint `Box` of `A` |
| `u8-callback-condition-instance` | condition must be boolean, got `(i32) => void` |
| `u8-minus-string-equality-instance` | operator not defined for `i32` and `string` |
| `u8-member-read-instance` | `i32` has no member `v` |
| `u8-member-write-instance` | `i32` has no member `v` |
| `u8-method-instance` | `i32` has no method `get` |
| `u8-call-instance` | type `i32` is not callable |
| `u8-add-string-instance` | operator not defined for `i32` and `string` |
| `u8-relational-string-instance` | operator not defined for `i32` and `string` |
| `u8-relational-boolean-instance` | operator not defined for `i32` and `boolean` |
| `u8-logical-or-instance` | logical operators require booleans, got `i32` |
| `u8-logical-and-instance` | logical operators require booleans, got `i32` |
| `u8-logical-or-mismatch-instance` | logical operators require booleans, got `i32` |
| `u8-logical-and-mismatch-instance` | logical operators require booleans, got `i32` |
| `u8-logical-right-mismatch-instance` | logical operators require booleans, got `i32` |
| `u8-logical-identity-instance` | logical operators require booleans, got `i32` |
| `u8-unary-bang-instance` | `!` requires a boolean operand, got `i32` |
| `u8-fixed-array-large-layout-instance` | `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes |
| `u8-array-nullable-element-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `u8-map-nullable-value-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `u8-map-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `u8-set-nullable-key-instance` | `i32 \| null` is not a Map/Set key kind; Q24 permits sized integers, boolean, enum, f32/f64, string, Date, and reference classes |
| `u8-array-find-instance` | `find` is rejected: A scalar element type has no miss value; use `findIndex` (Q22) |
| `u8-map-get-instance` | `get(key)` is rejected: A scalar value type has no null miss value; use `getOr` (Q24) |
| `u8-string-mismatch-instance` | type mismatch: the initializer expects `string`, got `i32` |
| `u8-constraint-to-parameter-instance` | type mismatch: the initializer expects `i32`, got `Box` |
| `u8-parameter-to-constraint-instance` | type mismatch: the initializer expects `Box`, got `i32` |
| `u8-nullable-declaration-instance` | unions are limited to `Ref \| null`; `i32 \| null` is not a reference type union |
| `u8-nullish-same-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `u8-nullish-box-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `u8-nullish-mismatch-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `u8-narrowing-instance` | operator not defined for `i32` and `null` |
| `u8-iteration-instance` | `for…of` accepts only T[], FixedArray<T, N>, Set, string, or Generator<T>; got `i32` |
| `u8-spread-instance` | array-literal spread accepts T[], FixedArray<T, N>, Set, or string; got `i32` |
| `u8-switch-string-instance` | type mismatch: the case label expects `i32`, got `string` |
| `u8-conditional-instance` | condition must be boolean, got `i32` |
| `u8-if-condition-instance` | condition must be boolean, got `i32` |
| `u8-string-compound-instance` | compound assignment is not defined for `string` |
| `u8-logical-and-right-instance` | logical operators require booleans, got `i32` |
| `u8-relational-class-instance` | operator not defined for `i32` and `Box` |
| `u8-logical-relational-instance` | logical operators require booleans, got `i32` |
| `u8-nullish-relational-instance` | the left operand of `??` has type `i32`, which is not nullable |
| `u8-while-condition-instance` | condition must be boolean, got `i32` |
| `u8-for-condition-instance` | condition must be boolean, got `i32` |
| `u8-assert-to-parameter-instance` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `Box` to `i32` |
| `u8-nested-constraint-instance` | type argument `i32` does not satisfy the constraint `Box` of `A` |
| `array-iteration-compound-instance` | compound assignment is not defined for `i32` |
| `array-s-add-x-instance` | compound assignment is not defined for `i32` |
| `array-s-subtract-x-instance` | compound assignment is not defined for `i32` |
| `array-s-bitand-x-instance` | compound assignment is not defined for `i32` |
| `array-field-add-x-instance` | compound assignment is not defined for `i32` |
| `array-callback-condition-instance` | condition must be boolean, got `(i32[]) => void` |
| `array-remainder-equality-instance` | operator not defined for `i32[]` and `i32` |
| `array-fresh-equality-instance` | operator not defined for `i32[]` and `i32` |
| `array-fresh-index-instance` | operator not defined for `i32[]` and `i32` |
| `array-fresh-bitand-instance` | operator not defined for `i32[]` and `i32` |
| `array-minus-string-equality-instance` | unary `-` requires a numeric operand, got `i32[]` |
| `nullable-iteration-compound-instance` | compound assignment is not defined for `i32` |
| `nullable-s-add-x-instance` | compound assignment is not defined for `i32` |
| `nullable-s-subtract-x-instance` | compound assignment is not defined for `i32` |
| `nullable-s-bitand-x-instance` | compound assignment is not defined for `i32` |
| `nullable-field-add-x-instance` | compound assignment is not defined for `i32` |
| `nullable-callback-condition-instance` | condition must be boolean, got `(Box) => void` |
| `nullable-remainder-equality-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-fresh-equality-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-fresh-index-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-fresh-bitand-instance` | operator not defined for `Box \| null` and `i32` |
| `nullable-minus-string-equality-instance` | unary `-` requires a numeric operand, got `Box \| null` |

### Final validation

`cargo fmt --check` passes. The final quick gate passes on the complete checker form.
It reports 2,145 passed tests, zero failed tests, three ignored tests, and two skips.
The 1,193-cell matrix and all 24 opaque-generic tests pass inside the gate.
Gate record: `target/gate/20261001T151529Z-quick.md`.

```text
gate quick 616eccd34085f4b5739814a16a9a79ecbdc26251 dirty:12 debug 2145/0/3 skips 2 goldens-moved 0 exit 0
```

No file-scope, corpus rejection, undecided-cell, or gate-failure stop occurs.
No `.expected` golden changes. All work stays uncommitted.

## Fix round landing gate

The orchestrator ran the full gate on the round 8 tree.

```text
gate full 616eccd34085f4b5739814a16a9a79ecbdc26251 dirty:12 debug 2145/0/3 release 2142/0/3 skips 2/0 clippy 4/18/13 goldens-moved 0 exit 0
```
