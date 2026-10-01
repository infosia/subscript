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

The 270 rows of this table are not kept here; this file at `d901355d` holds them. For the product cells, `SUBSCRIPT_MATRIX_OMISSIONS=<file> cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix` writes the current list, and the test pins the omitted count.

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

The 127 rows of this table are not kept here; this file at `d901355d` holds them. For the product cells, `SUBSCRIPT_MATRIX_OMISSIONS=<file> cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix` writes the current list, and the test pins the omitted count.

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


## Verification review fix round

The checker pin is `cf7cce94`. No commit is made.

### Findings and tests

| Finding | Fix | Test |
|---|---|---|
| Assignment to an intermediate constraint | `assignable` walks direct constraints by identity and keeps a cycle guard. | `a_parameter_is_assignable_through_each_intermediate_constraint`; the product's linked roles at all assignment sites |
| A parameter case label on a concrete discriminant | `check_case_label` uses `generic_overlap` when either operand involves a parameter. | `a_parameter_case_label_compares_with_a_concrete_discriminant`; product case sites on `T`, `i32`, and `string` |
| Direct constraint cycles | The opaque root checks every complete constraint chain and reports S100 at each affected declaration. | `direct_constraint_cycles_report_the_declaration`; two cyclic matrix cells; `r296` |
| Compound diagnostic text | The message names the operator and both apparent operand types. | `compound_diagnostics_name_the_operator_and_both_operands`; the existing diagnostic unit test |
| Equal string-addition branches | One condition joins the three string-result cases. | The matrix; the compiler clippy probe |

The intermediate-constraint test covers class and numeric constraints, both declaration orders, and three parameter levels.
It checks initialization, assignment, return, argument, and array `push`, with and without an instance.
A reverse assignment rejects. An unknown-name control fires S016 for each accepted body.
The case-label test keeps a rejected class-constraint control for both concrete discriminants.

Changed expectation: `check::expr::tests::compound_assignment_keeps_its_specific_type_diagnostic` in `compiler/src/check/expr.rs`.
The old text was ``compound assignment is not defined for `boolean` ``.
The new text is ``operator `+=` is not defined for `boolean` and `boolean` ``.
The S100 verdict and diagnostic count stay unchanged.
No other existing test pins the old message.

The clippy probe reports two library warnings and three library-test warnings, including the two duplicates.
Thus the compiler has three distinct library and library-test warnings.
The string-addition site reports no `if_same_then_else` warning.
Integration-test warnings are unchanged.

### Product matrix and cost

`generic_tsc_matrix/product.rs` builds the product. The existing hand-written forms stay as extra cells.
The seven roles are `T`, `T | null`, `x + 1`, `-x`, each linked declaration order, and a concrete value.
Each linked role has a constrained intermediate `U`. The unconstrained kind uses `U extends Box` for those roles.
The seven parameter kinds include `extends f64` and `extends u8`.

The 63 consumer sites include initialization, assignment, return, argument, and `push` for `U`, `T`, and `i32`.
They include all eleven compound operators on each target kind.
Equality and relational sites consume each operand side.
The other sites are switch discriminant, three case-label discriminants, condition, template, index, and two Map key types.

The generator creates `7 × 63 × 7 × 2 = 6,174` candidate product cells.
For an unconstrained instance, it tries `i32`, `boolean`, `string`, `Box`, and `i32[]` before an omission.
A constrained role uses an argument that satisfies its constraint.
Concrete controls share results only for identical source text.
The generic checker still checks each retained cell separately.
All 3,087 product cells without an instance remain.
The 1,477 omitted instance cells have rejected concrete controls. The table below lists each diagnostic.
No cell is removed to reduce test cost.

The concrete-condition divergence names S100 and `compiler.md §68`.
The record states that a `ConditionalBranch` condition is a `boolean` value.
The record check resolves that section through the compiler index and checks the restriction token.

| Matrix measurement | Result |
|---|---|
| Product candidates | 6,174 |
| Product retained / omitted instance cells | 4,697 / 1,477 |
| Hand-written extra cells | 1,199 |
| Total cells without / with an instance | 3,886 / 2,010 |
| Complete matrix at the pin | 155 failures / 5,896 cells, 5.913069250 s |
| Complete matrix after the fix | 0 failures / 5,896 cells, 5.903477834 s |
| First product prototype | 7 failures / 5,878 cells, 7.649746875 s |
| Prototype failures | Seven concrete-condition cells lacked their restriction record. |

The first product cost exceeded the five-second target. That cost was reported before any cost-based cut.
The proposal was to retain the product and share duplicate concrete controls.
This reduced the measured cost to 5.90 seconds; the cell set grew after alternative-argument checks.
An eight-second budget covers the complete product without a coverage cut.
The final cost adds 4.28 seconds to the previous 1.624-second matrix in a 25–55-minute debug gate.
The full product guards assignment and comparison pairs that a hand-written form list missed.
One TypeScript process checks the whole project.

### Red and rule 5 measurements

A CLI built from `git archive cf7cce94` accepts `r296-circular-type-parameter-constraint.ts` with no errors.
TypeScript 5.9.2 reports TS2313 for both constraints at line 8.
The options match `tsc_corpus.rs`, with the ambient language prelude.
The current corpus reject test requires S100 at line 8 and passes.
The generator updates `generated-docs/corpus-index.md`; no other generated document changes.

The rule 5 harness uses the corpus loaders' `entry_ids` and `entry_sources` functions.
It copies `trap_ids` and `trap_sources` from the trap loader, including all ambient mirrors.
It discovers every example recursively and uses the same loader plus the engine mirror.
The temporary harness is removed after the measurement.

| Corpus measurement | Result |
|---|---|
| Accept / warn / trap / examples | 294 / 5 / 71 / 17 |
| Programs / source files | 387 / 511 |
| Rejected at the pin / current | 0 / 0 |
| Pin checker time, one debug pass | 1.442712292 s |
| Current checker time, one debug pass | 1.431803000 s |
| Checker time change | -0.76% |

All 28 opaque-generic tests, the reject corpus tests, and the compound diagnostic unit test pass.
No public API or new deferral entry is added. No `.expected` golden changes.

### Changed Rust file sizes

| File | Lines |
|---|---:|
| `compiler/src/check/expr.rs` | 550 |
| `compiler/src/check/expr/operator.rs` | 1483 |
| `compiler/src/check/generics.rs` | 626 |
| `compiler/src/check/opaque.rs` | 603 |
| `compiler/src/check/stmt.rs` | 1529 |
| `compiler/src/check/type_rules.rs` | 269 |
| `compiler/tests/corpus_reject.rs` | 1324 |
| `compiler/tests/generic_tsc_matrix.rs` | 1517 |
| `compiler/tests/generic_tsc_matrix/product.rs` | 276 |
| `compiler/tests/opaque_generics.rs` | 963 |

Every changed Rust file stays below 2,000 lines.

### Product instance omissions

Each row lists every tried concrete type and its diagnostic message. All Markdown pipe characters are escaped.

The 1477 rows of this table are not kept here; this file at `d901355d` holds them. For the product cells, `SUBSCRIPT_MATRIX_OMISSIONS=<file> cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix` writes the current list, and the test pins the omitted count.


### Final validation

`cargo fmt --check` passes. The quick gate reports 2,149 passed tests, zero failures, and three ignored tests.
It reports two skips and zero changed goldens.
`tools/hygiene.sh` and `git diff --check` pass.
Gate record: `target/gate/20261001T155433Z-quick.md`.

```text
gate quick cf7cce9480bb85b8e47ed574d32e27e29c784abe dirty:13 debug 2149/0/3 skips 2 goldens-moved 0 exit 0
```

No file-scope, corpus-rejection, undecided-record, or gate-failure stop occurs.
The matrix exceeds the five-second target; its measured cost and eight-second budget are stated above.
No cell is removed for cost. All work stays uncommitted.

## Verification review fix round landing gate

The orchestrator ran the full gate on the round 9 tree.

```text
gate full cf7cce9480bb85b8e47ed574d32e27e29c784abe dirty:13 debug 2149/0/3 release 2146/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

The product matrix costs 5.90 s, above the 5 s aim of the handoff. No
cell was cut: the cost is 0.5 % of a full gate, and each cell is a
distinct pair of value role and consumer site.


## Final review fix round
Every value-shape test uses `apparent_type`, directly or through an immutable local that the same function resolves.
`apparent_expr` uses that function. Type identity, assignability, constraints, and storage layout retain their raw type form.
No per-site deferral flag was added. No test expectation or golden changed.

### Findings and tests

| Finding | Change | Test |
|---|---|---|
| Object and array patterns reject constrained parameters | `pattern_source_fits` reads the apparent source shape. | `derived-object-pattern`, `derived-parameter-pattern`, `derived-iteration-pattern`, `derived-narrowed-pattern`, `derived-array-pattern`; both columns. |
| Context-free ternaries lose parameter identity | `check_cond` uses `generic_union` when either arm involves a parameter. Assignment checks retain the union. | `derived-ternary-number`, `derived-ternary-class`, `derived-ternary-parameters`, `derived-ternary-number-operation`; both columns. |
| Context-free array literals reject concrete values beside T | The inferred element uses `generic_union`; concrete literal inference retains its context. | `derived-array-class`, `derived-array-number`; both columns. |
| GenericUnion receivers reject members and methods | Each member must supply the operation through its apparent type. The result preserves each member's type. | `derived-nullish-member`, `derived-nullish-new-member`, `derived-nullish-method`; both columns. |
| A union member lacks the requested property | Every branch is checked. Different declared classes retain C1 at concrete instantiation. | `derived-union-shared-member`; `every_union_member_must_supply_the_member` keeps a reject control. |
| Constrained Error operands fail S010 | `is_error_type` uses the apparent type. | `derived-throw-error`; both columns. |
| Unconstrained throw stays S010 | The matrix names `compiler.md §115` and its Error-family operand rule. | `derived-throw-unconstrained`; both columns. |
| Product instances disappear when controls reject | Each control runs once. The product asserts exactly 4,468 omitted instances. | `generic_forms_follow_tsc`; the full omitted list follows. |
| Arrow defaults skip assignability | The arrow checker checks each default against its declared parameter type. | Product default-parameter cells; `lambda_defaults_use_the_declared_parameter_type`. |
| Project restrictions use constrained shapes too early | The existing UnaryNumeric and TemplateInterpolation admission tests still defer f16 arithmetic and Date formatting. | `derived-f16-unary`; both columns. `derived-date-template`; no-instance only. |

All `derived-*` cells must pass TypeScript. The test reports a rejected TypeScript accept control separately.
The Date formatter's concrete control rejects under the existing Date template-interpolation restriction; its instance cell is omitted.
The shared-member instance names S005 and C1 because distinct concrete class types retain nominal identity.
The source scan routes variant equality, including Date and RegExp dispatch, through the same function.

### Total source check

`compiler/tests/apparent_type_shapes.rs::every_value_shape_test_uses_the_apparent_type` scans every Rust file under `compiler/src/check` recursively.
The scan reads token groups. It ignores comments, character literals, regular strings, and raw strings.
It reports Type patterns in `match`, `let`, and `matches!`, variant equality in both orders, and numeric or iteration predicates.
Each Type-bearing tuple member must have its own route. Immutable local routes follow lexical scope; a child scope cannot cover its parent.
A named allowlist group pins each raw function's input and pattern sequence with a fingerprint.
A new raw test changes that fingerprint, including a test inside an allowed function.
The test also rejects stale allowlist entries.

The scan reports 479 tests. The allowlist has 14 named groups and 42 fixed function fingerprints.
Its measured cost is 0.133 seconds. Six scanner tests include raw/routed controls, tuple coverage, scope coverage, equality orders, and fingerprint changes.

| Allowlist group | Reason |
|---|---|
| `concrete-captures` | Capture validation runs on the final concrete HIR after opaque instances leave it (§135 rule 1). |
| `union-members` | Union decomposition preserves T identity; each member then uses apparent_type (§143 rule 1c). |
| `instance-identity` | Instance keys compare declared type arguments and preserve T identity (§143 rule 1b). |
| `concrete-initializers` | Initializer effect analysis runs on the final concrete HIR (§135 rule 1). |
| `json-number` | The callers supply apparent_type to this scalar conversion-code selector. |
| `concrete-layout` | Layout validation runs after opaque instances leave the HIR (§135 rule 1). |
| `storage-layout` | Storage layout inspects the declared type form; T has no concrete layout (§143 rule 2a). |
| `mirror-declarations` | C mirror declarations have concrete boundary types and cannot declare type parameters. |
| `parameter-form` | These tests resolve or preserve T identity; they do not select a value operation (§143 rules 1a–1d). |
| `operation-signatures` | The operation dispatcher constructs these outer container, function, and Worker shapes before this pass. |
| `closed-alias` | The switch checker admits exhaustive aliases only from a declared StringAlias, never T. |
| `assignability` | Assignability and its diagnostics must inspect T itself (§143 rule 1b). |
| `wire-declarations` | Wire boundary declarations come from concrete mirror types, which cannot declare type parameters. |
| `signature-identity` | These declarations and return checks require exactly Void; constraint projection changes that identity (§143 rule 1b). |

The raw type-form groups implement §143 rules 1a–1d or rule 2a, rather than select an operation on a value.
The other groups inspect concrete declarations, final HIR, or dispatcher-built outer shapes that cannot be a type parameter.
The following table enumerates every scanned site by file and function. Counts include each Type-bearing match arm.

| File under `compiler/src/check` | Function | Routed tests | Allowlisted raw tests |
|---|---|---:|---:|
| `bindings.rs` | `is_context_affine_type` | 2 | 0 |
| `bindings.rs` | `pattern_source_fits` | 3 | 0 |
| `bodies.rs` | `check_function` | 2 | 0 |
| `bodies.rs` | `check_this_in_assignment_prefix` | 1 | 0 |
| `bodies.rs` | `require_field_values` | 1 | 0 |
| `capture.rs` | `expr` | 0 | 2 |
| `capture.rs` | `fact` | 0 | 4 |
| `capture.rs` | `type_name` | 0 | 9 |
| `class_shape.rs` | `plain_value_leaf` | 1 | 0 |
| `class_shape.rs` | `resolve_class_method` | 0 | 1 |
| `class_shape.rs` | `resolve_class_shape` | 4 | 0 |
| `class_shape.rs` | `validate_class_index_accessors` | 0 | 1 |
| `class_shape.rs` | `value_field_ok` | 2 | 0 |
| `container_argument.rs` | `enter_container_context` | 1 | 0 |
| `exception.rs` | `check_error_new` | 1 | 0 |
| `exception.rs` | `check_instanceof` | 1 | 0 |
| `exception.rs` | `check_throw` | 1 | 0 |
| `exception.rs` | `is_error_type` | 1 | 0 |
| `exception.rs` | `visit` | 2 | 0 |
| `expr.rs` | `contextual_object_class` | 3 | 0 |
| `expr/aggregate.rs` | `check_array_lit` | 3 | 0 |
| `expr/aggregate.rs` | `check_array_spread_lit` | 7 | 0 |
| `expr/aggregate.rs` | `check_descriptor_lit` | 1 | 0 |
| `expr/array_of_and_map_copy.rs` | `check_array_of` | 1 | 0 |
| `expr/array_of_and_map_copy.rs` | `check_map_copy` | 4 | 0 |
| `expr/assign.rs` | `check_assign` | 1 | 0 |
| `expr/assign.rs` | `check_member_place` | 3 | 0 |
| `expr/call.rs` | `check_args` | 1 | 0 |
| `expr/call.rs` | `check_context_bytes_call` | 4 | 0 |
| `expr/call.rs` | `check_indirect_call` | 2 | 1 |
| `expr/call.rs` | `check_method_call` | 1 | 0 |
| `expr/call.rs` | `check_method_call_on` | 15 | 1 |
| `expr/call.rs` | `check_new` | 2 | 0 |
| `expr/call.rs` | `check_set_source` | 4 | 0 |
| `expr/call.rs` | `context_bytes_storage_rejection` | 4 | 0 |
| `expr/entry.rs` | `check_await` | 5 | 0 |
| `expr/entry.rs` | `embedded_header_projection` | 2 | 0 |
| `expr/entry.rs` | `reject_embedded_header_copy` | 1 | 0 |
| `expr/lambda.rs` | `check_lambda` | 1 | 0 |
| `expr/lambda.rs` | `check_lambda_with` | 2 | 0 |
| `expr/literal.rs` | `apply_narrowing` | 1 | 0 |
| `expr/literal.rs` | `check_ident` | 1 | 0 |
| `expr/literal.rs` | `check_lit` | 1 | 0 |
| `expr/literal.rs` | `check_num_lit` | 3 | 0 |
| `expr/literal.rs` | `check_template` | 3 | 0 |
| `expr/member.rs` | `check_index` | 4 | 0 |
| `expr/member.rs` | `check_member_read_inner` | 1 | 0 |
| `expr/member.rs` | `is_absence_capable_member_expr` | 1 | 0 |
| `expr/member.rs` | `member_on` | 12 | 1 |
| `expr/method.rs` | `check_array_from` | 5 | 0 |
| `expr/method.rs` | `check_array_method` | 9 | 0 |
| `expr/method.rs` | `check_map_group_by` | 6 | 1 |
| `expr/method.rs` | `check_map_method` | 3 | 0 |
| `expr/method.rs` | `check_number_method` | 3 | 0 |
| `expr/method.rs` | `check_set_method` | 2 | 0 |
| `expr/method.rs` | `expect_callback_shape` | 3 | 0 |
| `expr/method.rs` | `map_get_value_ok` | 1 | 0 |
| `expr/method.rs` | `reduce_acc_context` | 2 | 0 |
| `expr/namespace.rs` | `check_date_new` | 1 | 0 |
| `expr/namespace.rs` | `check_receiver` | 1 | 0 |
| `expr/namespace.rs` | `check_string_pattern_method` | 2 | 0 |
| `expr/namespace.rs` | `check_worker_spawn` | 3 | 1 |
| `expr/namespace.rs` | `non_transferable_message_field` | 5 | 0 |
| `expr/operator.rs` | `bin_result` | 33 | 0 |
| `expr/operator.rs` | `check_as` | 13 | 0 |
| `expr/operator.rs` | `check_bin` | 5 | 0 |
| `expr/operator.rs` | `check_cond` | 1 | 0 |
| `expr/operator.rs` | `check_nullish` | 2 | 0 |
| `expr/operator.rs` | `check_optional_chain_statement` | 1 | 0 |
| `expr/operator.rs` | `check_optional_member` | 1 | 0 |
| `expr/operator.rs` | `check_optional_plan` | 1 | 0 |
| `expr/operator.rs` | `check_unary` | 8 | 0 |
| `expr/operator.rs` | `check_update` | 4 | 0 |
| `expr/operator.rs` | `finish_nullish_plan` | 2 | 0 |
| `expr/operator.rs` | `nullish_result_type` | 1 | 0 |
| `expr/operator.rs` | `reject_unbound_optional_chain` | 1 | 0 |
| `expr/operator.rs` | `require_nullable_operand` | 1 | 0 |
| `generics.rs` | `numeric_normal_form` | 0 | 14 |
| `generics.rs` | `same_normal_form` | 0 | 4 |
| `host_entries.rs` | `populate` | 0 | 1 |
| `init_effects.rs` | `class_of` | 0 | 2 |
| `instance_chain.rs` | `argument_has_error` | 0 | 2 |
| `json.rs` | `check_json_call` | 1 | 0 |
| `json.rs` | `check_json_parse` | 3 | 0 |
| `json.rs` | `collect_json_types` | 2 | 0 |
| `json.rs` | `json_array_construction_body` | 4 | 0 |
| `json.rs` | `json_array_validation_body` | 1 | 0 |
| `json.rs` | `json_construction_body` | 8 | 0 |
| `json.rs` | `json_helper_body` | 14 | 0 |
| `json.rs` | `json_number_target` | 0 | 10 |
| `json.rs` | `json_validation_body` | 5 | 0 |
| `json.rs` | `visit` | 9 | 0 |
| `json.rs` | `walk` | 2 | 0 |
| `layout.rs` | `expression_builds_into_destination` | 0 | 2 |
| `layout.rs` | `has_managed_interior` | 0 | 2 |
| `layout.rs` | `independent_type_layout` | 0 | 3 |
| `layout.rs` | `is_aggregate` | 0 | 2 |
| `layout.rs` | `type_layout` | 0 | 3 |
| `layout.rs` | `validate_expr_frame` | 0 | 1 |
| `mirror_provenance.rs` | `foreign_parameter_provenance` | 0 | 6 |
| `opaque.rs` | `constrain_opaque_param` | 0 | 2 |
| `opaque.rs` | `constraint_cycle` | 0 | 1 |
| `opaque.rs` | `direct_constraint` | 0 | 1 |
| `opaque.rs` | `generic_overlap` | 0 | 8 |
| `opaque.rs` | `generic_union` | 0 | 1 |
| `opaque.rs` | `instance_restriction` | 15 | 0 |
| `opaque.rs` | `involves_type_parameter` | 0 | 2 |
| `opaque.rs` | `is_type_parameter` | 0 | 1 |
| `opaque.rs` | `is_unconstrained_type_parameter` | 0 | 1 |
| `opaque.rs` | `non_null_type` | 0 | 2 |
| `opaque.rs` | `resolve_apparent_type` | 0 | 5 |
| `pipeline.rs` | `normalize_operation_parameter_types` | 0 | 6 |
| `pipeline.rs` | `visit_expr` | 0 | 1 |
| `signatures.rs` | `resolve_mirror_signatures` | 0 | 5 |
| `stmt.rs` | `check_bindings` | 6 | 0 |
| `stmt.rs` | `check_for_of` | 2 | 0 |
| `stmt.rs` | `check_for_of_subject_under_flag` | 5 | 0 |
| `stmt.rs` | `check_return` | 2 | 2 |
| `stmt.rs` | `check_switch` | 3 | 0 |
| `stmt.rs` | `for_of_subject_from` | 5 | 0 |
| `stmt.rs` | `is_fused_view_receiver` | 1 | 0 |
| `stmt.rs` | `narrow_paths` | 1 | 0 |
| `stmt.rs` | `require_bool` | 1 | 0 |
| `stmt.rs` | `stmt_returns` | 0 | 1 |
| `type_rules.rs` | `assignable_through_constraints` | 1 | 17 |
| `type_rules.rs` | `contains_string_alias` | 0 | 4 |
| `type_rules.rs` | `is_value_class` | 1 | 0 |
| `type_rules.rs` | `report_not_assignable` | 0 | 8 |
| `type_rules.rs` | `supported_wire_alias_boundary_type` | 0 | 3 |
| `tyres.rs` | `resolve_type_ref` | 4 | 0 |
| `tyres.rs` | `resolve_union` | 1 | 0 |

### Product and cost

| Axis or count | Result |
|---|---:|
| Value roles | 20 |
| Consumer sites | 74 |
| Parameter kinds | 7 |
| Instance columns | 2 |
| Product candidates | 20,720 |
| Product no-instance cells | 10,360 |
| Product admitted instance cells | 5,892 |
| Product omitted instance cells | 4,468 |
| Product cells | 16,252 |
| Kept hand-written and finding cells | 1,236 |
| Total no-instance cells | 11,178 |
| Total instance cells | 6,310 |
| Total cells | 17,488 |
| Failures | 0 |
| Final measured matrix time | 22.173609084 s |

The first expanded product cost 44.667767875 seconds: 17,205 cells, 268 failures, and 4,714 omitted instances.
It had 20 roles, 74 sites, seven kinds, and two columns. That exceeded ten seconds and was reported before further work.
No cell was removed for cost. Each product cell is a distinct pair of value role and consumer site.
The generator emits only the helpers each role needs. It checks product admission controls once, rather than twice.
The matrix states a 30-second budget. TypeScript runs once over all admitted cells in one temporary project.
Field-initializer cells use a declared generic field and name S100/C9 for its prohibited `this` read.
Default-parameter cells use a const source value, so the closure's capture rule does not hide default-value assignability.
The field and default-parameter cells preserve all 20 roles and seven kinds.

### Rule 5 measurement

The harness uses `codegen/tests/corpus/mod.rs::entry_ids` and `entry_sources`.
It copies the trap loaders from `codegen/tests/support/trap_corpus.rs`, including all ambient mirrors.
It discovers every example recursively and adds its required engine mirror.
The same harness runs against HEAD `1e901ca9` and the final checker. The temporary test is removed after measurement.

| Measurement | Result |
|---|---:|
| Programs | 387 |
| Sources | 511 |
| Rejected programs at the pin | 0 |
| Rejected programs after the change | 0 |
| Pin debug checker time | 1.438187333 s |
| Final debug checker time | 1.411691125 s |
| TypeScript version | 5.9.2 |
| TypeScript unconstrained throw verdict | exit 0, no diagnostic |

TypeScript uses the same strict, ES2022, ESNext.Disposable, Bundler, noEmit, and empty-types options as the matrix.
The library clippy check retains its two existing argument-count warnings; the existing library-test warning makes the prior count three.

### Changed Rust file sizes

| File | Lines |
|---|---:|
| `compiler/src/check/bindings.rs` | 250 |
| `compiler/src/check/bodies.rs` | 845 |
| `compiler/src/check/class_shape.rs` | 979 |
| `compiler/src/check/container_argument.rs` | 93 |
| `compiler/src/check/exception.rs` | 591 |
| `compiler/src/check/expr.rs` | 553 |
| `compiler/src/check/expr/aggregate.rs` | 400 |
| `compiler/src/check/expr/array_of_and_map_copy.rs` | 134 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1555 |
| `compiler/src/check/expr/entry.rs` | 679 |
| `compiler/src/check/expr/lambda.rs` | 248 |
| `compiler/src/check/expr/literal.rs` | 538 |
| `compiler/src/check/expr/member.rs` | 590 |
| `compiler/src/check/expr/method.rs` | 1570 |
| `compiler/src/check/expr/namespace.rs` | 1282 |
| `compiler/src/check/expr/operator.rs` | 1505 |
| `compiler/src/check/json.rs` | 1597 |
| `compiler/src/check/lookup.rs` | 212 |
| `compiler/src/check/opaque.rs` | 624 |
| `compiler/src/check/signatures.rs` | 552 |
| `compiler/src/check/stmt.rs` | 1538 |
| `compiler/src/check/type_rules.rs` | 269 |
| `compiler/src/check/tyres.rs` | 678 |
| `compiler/tests/apparent_type_shapes.rs` | 640 |
| `compiler/tests/generic_tsc_matrix.rs` | 1544 |
| `compiler/tests/generic_tsc_matrix/findings.rs` | 60 |
| `compiler/tests/generic_tsc_matrix/product.rs` | 483 |

Each changed Rust file stays below 2,000 lines. No source outside the authorized compiler and tracking-note file set changes.
No corpus-rejection or file-scope stop occurs. The quick gate verdict is recorded below.

### Omitted product instances

Each concrete candidate rejects outside the accepted body form. These 4,468 cells stay outside the instance column under §143 rule 4.
The asserted count detects an additional omission. Each reason is the diagnostic message, with Markdown pipes escaped.

The 4468 rows of this table are not kept here; this file at `d901355d` holds them. For the product cells, `SUBSCRIPT_MATRIX_OMISSIONS=<file> cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix` writes the current list, and the test pins the omitted count.


### Final validation

`cargo fmt --check` passes. The quick gate reports 2,157 passed tests, zero failures, and three ignored tests.
It reports two skips and zero changed goldens. `tools/hygiene.sh` and `git diff --check` pass.
Gate record: `target/gate/20261001T170728Z-quick.md`.

```text
gate quick 1e901ca96f6856195cfafa856a92eb8b002fe35d dirty:29 debug 2157/0/3 skips 2 goldens-moved 0 exit 0
```

No file-scope, corpus-rejection, undecided-record, or gate-failure stop occurs.
The matrix exceeds ten seconds; its axis counts and measured cost are stated above. No cell is removed for cost.
All changes stay uncommitted.

## Final review fix round landing gate

The orchestrator ran the full gate on the round 10 tree.

```text
gate full 1e901ca96f6856195cfafa856a92eb8b002fe35d dirty:29 debug 2157/0/3 release 2154/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

The matrix costs 22.17 s in debug, above the 10 s bound of the round
handoff; the round did not report it before landing. The cost is open:
see the next review.


## Final review round 2 fix round

### Findings, fixes, and tests

1. A type-parameter argument now follows its direct constraints before numeric normalization.
   The check preserves parameter identity and uses a cycle guard (§143 rule 1b).
   Both concrete types then use the same numeric normal form (§135 rule 2b).
   The matrix adds forward type arguments and constrained class field types for array, Map, and both function constraints.
   Each derived control has both instance columns.
2. A callable class field now uses `apparent_type` before `function_type`.
   The matrix has class fields constrained to `() => i32` and `(x: i32) => void`, with both columns.
3. The shape scan derives its helper set from `compiler/src/types.rs` token groups.
   Each inherent `Type` method with `match self` or `matches!(self, ...)` enters the set.
   Each free function that takes `&Type` and matches that parameter enters the set.
   Reference lifetimes and argument positions are supported. Comments and strings do not count.
   The new-helper unit test declares a new method and a free function in source text.
   Both raw calls fail the route check; both apparent-type calls pass it.
4. The product keeps `+=`, `-=`, and `&=`, one operator per checker arm.
   A separate 99-cell test covers all 11 compound operators for each of the nine kinds.
   TypeScript starts before the checker loop. A reader thread drains its output during that loop.
   Up to eight scoped workers check disjoint cell slices. Results retain the cell order.

The expanded product also found a forward constraint that disappeared during a nested generic call.
The opaque identity table now retains each constraint when a callee replaces the caller's substitution map.
The forward-order roles test this form at both new constraint sites for all nine kinds.
No per-site deferral flag is added.

Computed callable conditions that TypeScript accepts name S100 and `compiler.md §68`, which requires a boolean condition.
A callable constraint admits no concrete boolean condition, so those instance cells stay omitted.
Their no-instance cells retain the restriction record. TypeScript-rejected callable conditions retain their checker errors.

The initial added controls measured 20 failing cells: 16 constraint cells and four callable-field cells.
All 20 controls now pass. The final matrix has zero failures.

### Additional helper scan sites

| File and function | Disposition |
|---|---|
| `bodies.rs::bind_params` | Route `carries_async_handle` through the apparent type. |
| `expr/assign.rs::check_assign` | Route `carries_async_handle` through the apparent type. |
| `expr/call.rs::check_method_call_on` | Route `function_type` through the apparent type. |
| `expr/entry.rs::track_async_call_result` | Route `carries_async_handle` through the apparent type. |
| `expr/literal.rs::check_num_lit` | Route `int_bounds` through the apparent type. |
| `expr/method.rs::map_get_value_ok` | Route `handle_kind` through the apparent type. |
| `tyres.rs::resolve_union` | Route `is_reference_shape` through the apparent type. |
| `type_rules.rs::is_reference_class` | Route `uses_reference_identity` through the apparent type. |
| `expr.rs::synthesized_int_range` | Allow: its literal caller supplies a concrete apparent numeric target. |
| `generics.rs::same_normal_form` | Allow: `contained_types` preserves declared argument identity. |
| `generics.rs::satisfies_constraint_through_parameters` | Allow: direct constraint steps preserve parameter identity. |
| `instance_chain.rs::argument_has_error` | Allow: `contained_types` inspects declared instance arguments. |
| `layout.rs::independent_type_layout` | Allow: `scalar_size_align` inspects declared storage layout. |
| `layout.rs::is_managed` | Allow: concrete HIR allocation analysis excludes opaque instances. |
| `opaque.rs::involves_type_parameter` | Allow: `contained_types` preserves parameter identity. |
| `type_rules.rs::type_name` | Allow: `display_type` preserves declared diagnostic names. |

The identity-table lookup removes the raw `direct_constraint` shape test and its obsolete allowlist entry.
The final scan covers 495 sites. Its 16 named groups pin 45 raw-test functions.
Seven scan tests pass. The final scan costs 0.204824750 seconds.

### Matrix cost

The review measured identical outcomes for the eight removed compound operators in all 633 groups.
The following step measurements use the same 20 roles and seven kinds, with the added derived regression controls.
They use one test thread; the last step creates scoped checker workers inside that test.
Each measurement includes cell construction, TypeScript, and checker work. Temporary project deletion follows the reported time.

| Step | Cells | Failures | Reported time |
|---|---:|---:|---:|
| Fixed checker, full operator product | 17,508 | 0 | 22.215761250 s |
| Keep one operator per arm | 12,444 | 0 | 15.938176000 s |
| Start TypeScript before the checker loop | 12,444 | 0 | 13.697495125 s |
| Split the checker loop with scoped workers | 12,444 | 0 | 6.465620625 s |
| Add function kinds and constraint/call sites | 16,000 | 0 | 9.448321167 s |
| Separate full compound-operator test | 99 | 0 | 0.235565417 s |

The final product has 20 roles × 54 sites × nine kinds × two columns = 19,440 candidates.
It omits 4,696 instance candidates whose concrete controls reject. The test pins that omitted count.
The retained product has 14,744 cells. The derived and existing form controls add 1,256 cells.
The final columns contain 10,548 no-instance cells and 5,452 instance cells.
The nine kinds include both function constraints beside the seven existing kinds.

The final test costs 10.40 seconds with temporary project deletion, above ten seconds.
This cost and the axis counts were reported before any further cut. No further cell is cut.
The full operator test costs 0.24 seconds. The full set keeps every operator and each distinct role/site pair.
The step measurements reduce the original product cost by 70.9 percent before the added coverage.

### Rule 5 corpus measurement

The harness uses the corpus loaders' `entry_ids` and `entry_sources` functions.
It copies `trap_ids` and `trap_sources` directly from the trap loader, including all ambient mirrors.
It discovers examples recursively and adds their required engine mirror.
The same temporary harness runs against HEAD `d901355d` and the final checker; it is removed after measurement.

| Measurement | Result |
|---|---:|
| Programs | 387 |
| Sources | 511 |
| Pin rejected programs | 0 |
| Final rejected programs | 0 |
| Pin debug checker time | 1.448747417 s |
| Final debug checker time | 1.407783958 s |
| Checker time change | -2.83% |
| TypeScript version | 5.9.2 |

No public API is added. No existing test verdict or corpus header changes.
No golden changes. No file-scope, corpus-rejection, or undecided-record stop occurs.

### Changed Rust file sizes

| File | Lines |
|---|---:|
| `compiler/src/check/bodies.rs` | 845 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1553 |
| `compiler/src/check/expr/entry.rs` | 679 |
| `compiler/src/check/expr/literal.rs` | 539 |
| `compiler/src/check/expr/method.rs` | 1571 |
| `compiler/src/check/generics.rs` | 651 |
| `compiler/src/check/opaque.rs` | 627 |
| `compiler/src/check/type_rules.rs` | 270 |
| `compiler/src/check/tyres.rs` | 680 |
| `compiler/tests/apparent_type_shapes.rs` | 769 |
| `compiler/tests/generic_tsc_matrix.rs` | 1591 |
| `compiler/tests/generic_tsc_matrix/findings.rs` | 104 |
| `compiler/tests/generic_tsc_matrix/product.rs` | 541 |

### Additional omitted instance cells

The previous omission tables retain the unchanged rows. These rows cover the added roles, sites, and kinds.
Each row states the concrete control diagnostic. The test pins the complete omitted count to 4,696.

The 1884 rows of this table are product cells and are not kept here. For the product cells, `SUBSCRIPT_MATRIX_OMISSIONS=<file> cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix` writes the current list, and the test pins the omitted count.

### Final validation

`cargo fmt --check` passes. The quick gate reports 2,159 passed tests, zero failures, and three ignored tests.
The debug test step costs 304 seconds. It reports two skips and zero changed goldens.
`tools/hygiene.sh` and `git diff --check` pass.
Gate record: `target/gate/20261001T175522Z-quick.md`.

```text
gate quick d901355d8e34bf47aec1f238a097b0dd1fd39f09 dirty:15 debug 2159/0/3 skips 2 goldens-moved 0 exit 0
```

No gate-failure stop occurs. All changes stay uncommitted.

## Final review round 2 landing gate

The orchestrator ran the full gate on the round 11 tree.

```text
gate full d901355d8e34bf47aec1f238a097b0dd1fd39f09 dirty:15 debug 2159/0/3 release 2156/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The orchestrator removed the omitted-cell tables from this note
(8,226 rows); each section states where its rows are.
