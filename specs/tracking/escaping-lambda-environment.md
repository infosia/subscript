# Escaping lambda environment

Contract: compiler.md §274 rules 1–6.

## Implementation

The parameter stores one `EscapeKind`: `None`, `Managed`, or `CCallback`.
The checker joins the kinds through parameter calls. `CCallback` dominates `Managed`.
Managed boundaries select owned environments through local aliases, assignments, and captured carriers.
A non-escaping parameter keeps its argument's stack environment.
C callback boundaries use capture facts that include owned lambdas. Their S009 diagnostics stay active.
A direct capture of `this` keeps the existing receiver restriction.
The reload signature includes the parameter kind.

Step 0: the existing interpreter, development JIT, and C AOT support the owned synchronous environment.
No new lowering form is necessary.
A function type cannot be a Set key. The checker test uses a Set of class instances that contain function fields.

## Round 1 measurements

The contract-pin checker rejects a446 at line 13, column 10:

```text
error[S009]: value of type `P` may capture at return
```

A returned lambda, field, array, Map value, and constructor parameter use the existing owned environment lowerings.
A separate HIR array probe prints `7` in the interpreter (0.019 seconds).
`Set<() => i32>` fails with S014 because a function type is not a permitted key kind.
The prototype passes a446 in the interpreter, development JIT, C AOT, and Node.
The memory control and owned form each retain 22 bytes after explicit collection.
Twenty iterations also retain 22 bytes.

A parameter that stores a C callback and a parameter that stores a managed function each recorded `escapes: true`.
The prototype accepted a capturing argument through the C callback parameter.
The C callback regression failed in 0.05 seconds.
Rule 6 supplies the destination kind that resolves that ambiguity.

## Round 1 results

Prototype results below describe the removed prototype. Times are test durations unless stated otherwise.

| Check | Result | Seconds |
|---|---|---:|
| C callback parameter regression, prototype | failed: the checker accepts the capture | 0.05 |
| Compiler library, prototype | 612 passed | 2.31 |
| Accept corpus, prototype | 13 passed | 3.13 |
| Reject corpus after candidate retirements, prototype | 49 passed | 0.97 |
| Ownership flag tests, prototype | 3 passed | 0.03 |
| Parameter capture tests before the new regression, prototype | 31 passed | 0.03 |
| Generated API reference witnesses, prototype | 1 passed | 1.01 |
| a446 three-tier and memory tests, prototype | 2 passed | 0.35 |
| Comparable Node corpus, prototype | passed | 0.61 |
| Project tsc, prototype | passed, wall time | 0.65 |
| Clippy, prototype | passed; no warnings in changed files, wall time | 3.67 |
| Collision index after candidate retirements, prototype | failed: owner C5 edit pending | 0.06 |


## Retired reject entries

Each source now passes. Its rejection registration and source file are removed, as for existing retired entries.
The owner must mark these entries `retired:` in C5. This task does not edit `specs/blocks/`.

- `retired:r10-escaping-capture`
- `retired:r240-parameter-stored-in-field`
- `retired:r241-parameter-returned`
- `retired:r242-expression-body-returns-parameter`
- `retired:r243-capture-passed-to-generator`
- `retired:r244-capture-passed-to-async`
- `retired:r245-yield-returns-capture`
- `retired:r246-capture-through-loop-back-edge`
- `retired:r247-generator-parameter-stored`
- `retired:r248-capture-through-indirect-call`

The new r502 entry and the compiler regression keep a capturing C callback rejected.
The regression also checks transitive propagation and a parameter that reaches both destinations.
No existing golden changes. The new a446 golden records the required output.
Generated references use `generate-api-reference`.

## Round 2 results

Times below are test durations unless the row says wall time.

| Check | Result | Seconds |
|---|---|---:|
| Compiler library | 612 passed | 2.16 |
| Accept corpus | 13 passed | 3.09 |
| Reject corpus | 49 passed | 1.32 |
| Counted capture blocks | 15 passed | 0.02 |
| C callback regressions | 2 passed | 0.05 |
| Ownership flag tests | 2 passed | 0.01 |
| Parameter capture tests | 32 passed | 0.04 |
| Combined compiler checks, wall time | passed | 12.757 |
| Generated API reference witnesses, wall time | passed | 7.797 |
| a446 three tiers and memory control | 2 passed | 0.34 |
| Reload tests | 39 passed | 0.56 |
| a446 Node comparison, wall time | passed | 0.151 |
| Project tsc, wall time | passed | 0.651 |
| JS corpus suite | 10 passed; collision index fails pending owner C5 update | 0.60 |
| JS corpus suite, wall time | collision index fails | 2.425 |
| cargo fmt --check, wall time | passed | 2.006 |
| Clippy, wall time | passed; no warnings in changed files | 6.612 |
| git diff --check, wall time | passed | 0.035 |

The new memory test compares a control with no owned environments against one and twenty runs of a446.
All three forms retain 22 live bytes after explicit collection. Owned environments add zero live bytes.
The JS collision index reports exactly the ten retired entries above. The comparable Node corpus test passes.
Node version: 24.22.1. TypeScript version: 5.9.2.
No commit, push, gate.sh run, or spec block change occurs.
No background task remains.

## Changed files

- `codegen/src/reload.rs`
- `codegen/tests/escaping_lambda_environment.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/capture/blocks.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/promise_reaction/helper.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/diag.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/parameter.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/counted_capture_block.rs`
- `compiler/tests/escaping_lambda_c_callback.rs`
- `compiler/tests/escaping_lambda_environment.rs`
- `compiler/tests/parameter_capture.rs`
- `corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.expected`
- `corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.ts`
- `corpus/reject/r10-escaping-capture.ts`
- `corpus/reject/r240-parameter-stored-in-field.ts`
- `corpus/reject/r241-parameter-returned.ts`
- `corpus/reject/r242-expression-body-returns-parameter.ts`
- `corpus/reject/r243-capture-passed-to-generator.ts`
- `corpus/reject/r244-capture-passed-to-async.ts`
- `corpus/reject/r245-yield-returns-capture.ts`
- `corpus/reject/r246-capture-through-loop-back-edge.ts`
- `corpus/reject/r247-generator-parameter-stored.ts`
- `corpus/reject/r248-capture-through-indirect-call.ts`
- `corpus/reject/r502-a-capturing-lambda-is-not-a-c-callback.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/tracking/escaping-lambda-environment.md`

## Round 3 Step 0

Form A does not preserve a distinct callback type.
An ambient plain function alias has the same signature as `SubLogCallback`, without callback metadata.
The checker accepts assignment from `SubLogCallback` to that alias and back.
Both aliases resolve to the same function type. Storage of the callback alias does not establish clean provenance.

Form B rejects 16 of 434 current accept entries. The unchanged checker accepts all 434.
The measurement includes 403 single-file entries and 31 directory entries, with corpus module options and interop mirrors.
The prototype treats each non-lambda, non-named-function value at a C boundary as potentially capturing.
It permits null, zero, and casts of clean values. Indirect call arguments use the C callback destination kind.

Rejected entries: `a25`, `a26`, `a27`, `a28`, `a30`, `a35`, `a39`, `a90`,
`a244`, `a245`, `a246`, `a247`, `a248`, `a249`, `a347`, and `a354`.
For example, a90 reports S009 for `info` at the C callback boundary.

The prototype also fails five existing tests:

- `interop_mirror::using_program_type_checks_against_the_generated_mirror`
- `parameter_capture::builtin_callbacks_stay_downward`
- `parameter_capture::indirect_calls_accept_owned_arguments`
- `escaping_lambda_c_callback::a_c_callback_parameter_rejects_a_capturing_lambda`
- `escaping_lambda_c_callback::c_callback_kind_dominates_managed_storage_and_propagates`

The last two tests reject the capture but fail their accepted clean controls.
The other interop tests pass: 13 mirror tests and one discovery test.
Thirty parameter capture tests pass.

Neither measured form meets the handoff condition. Step 0 stops the implementation.
The checker needs a distinct callback type that forbids capture through every conversion, or storage provenance that preserves clean values.
The prototype and temporary measurement test are removed. The prior implementation stays unchanged.
Only this tracking note changes in this round. The minor items and ship debug-runtime run remain unstarted.
No commit, push, or gate.sh run occurs.

| Measurement | Result | Wall seconds |
|---|---|---:|
| Complete baseline and bidirectional alias probe | 434 accepted; alias round trip accepted | 4.14 |
| Complete provenance prototype and alias probe | 418 accepted; 16 rejected; alias round trip accepted | 4.88 |
| Prototype interop, parameter, and single-file measurements | 3 test failures; 16 corpus rejections | 4.97 |
| Prototype C callback regression controls | 2 test failures | 3.51 |
| Restored checker, interop, parameter, C callback, and single-file measurements | 50 tests passed; 403 entries accepted | 6.80 |

No background task remains.

## Round 4: nominal callback type (F)

This round measures a form that contradicts the present structural callback alias rule.
The prototype adds a typedef identity to `FuncType` for each mirror callback provenance record.
A value place converts a named function or a lambda with no captures to that identity.
A plain function value cannot cross the identity in either direction.
The prototype changes nine Rust files, with 56 added or removed lines before the final provenance filter.
This is a checker measurement, not a complete implementation of nominal types.
Generic substitution and all nested type relations need an identity audit before implementation.
No runtime or tier change is necessary for the measured representation.

All 434 accept entries pass: 403 files and 31 directory programs.
No accept entry needs a source change. Thus the list of changed accept entries is empty.
Stock TypeScript 5.9.2 accepts the complete project with `node node_modules/typescript/bin/tsc -p tsconfig.json`.
The five controls pass: named function, clean lambda, refused plain input, refused plain output, and refused capture.
The controls use a separate boundary callback typedef with one `i32` parameter.
The complete accept sweep and the five controls take 3.17 test seconds.

The complete reject sweep checks 458 registered diagnostic pins and finds one changed pin in 2.19 test seconds.
`r502` still rejects, but S100 replaces S009 at line 10, column 25.
The pattern is realistic: a helper accepts a callback, then stores it for C.
The input lambda captures a local value. Nominal conversion rejects it before escape analysis supplies S009.
No reject entry becomes accepted in this sweep.
The interop mirror tests pass 14/14; discovery passes 1/1.

The largest F risk is identity erasure through a type operation that still assumes structural function types.
A production change must preserve typedef identity through aliases, nullable forms, containers, substitution, casts, and reload signatures.
Diagnostics must print the callback typedef name; the prototype currently prints two identical structural signatures on a mismatch.
Recommendation: prefer F when the owner wants to preserve the no-capture C boundary.
Its measured accept cost is zero, and its conversion rule separates plain storage from callback storage.
The prototype alone does not establish soundness for every type operation.

## Round 4: environment-owning binding (D)

D is a source review only. No D prototype, corpus rewrite, or tier execution occurs.
There is no measured D break count. The existing F measurements do not predict D test results.

Interning means that a repeated binding identity returns one stable record pointer.
Section 14.4a defines the observable identity as `(code, userdata1, userdata2)`.
A null environment makes this identity complete: there is no other per-closure state.
The implementation already keys on `(code, env, userdata1, userdata2)`.
Thus null is not a hash-map requirement; it is the premise of the current identity and bounded-growth contract.
Distinct owned environments require distinct records, even when code and userdata match.
Interning by the old three fields would incorrectly merge closures with different captures.
Interning by all four fields preserves those captures but grows with distinct environments.
A fresh capturing callback in a per-frame host registration is a realistic growth pattern.

`CallbackBinding` already stores `env`; `bind_callback` already receives it.
The Context trampoline copies it and `fire_callback` calls `f(ctx, env, ...)`.
The explicit registration also stores and forwards it through the same fire body.
Thus the call ABI can deliver an owned environment without a new trampoline argument.
Removing the two null assertions alone is insufficient.
`context/memory.rs` roots only the two userdata fields of each default binding.
`registration_roots` also roots only userdata, with the `open || active_calls > 0` predicate.
Both root sources must include the owned environment and its traced capture graph.
The checker must select an owned environment even for captures through a callback carrier.
A stack environment must never reach either binding path.

A default binding ends only when its Context ends.
Its environment must remain a root for that entire interval, even after native code stops its callbacks.
An explicit registration ends after host release and the last active call returns.
Release inside a callback must keep the environment rooted until that callback returns.
Removal drops a root; the next explicit collection reclaims an otherwise unreachable environment and its captures.
Release does not itself collect. Context destruction frees all remaining records and managed storage.
The host must stop notification sources before destruction, as section 111 requires.
An environment that remains reachable from script survives registration release.

Estimated D scope: two root enumerators, two null assertions, identity documentation, and environment liveness rules in the runtime.
The public binding ABI and record layout need no new field.
Compiler work must change callback escape classification and environment ownership selection.
The development JIT and C emitter already pass both code and environment to bind/register.
They need lifetime and collection tests, not a new boundary calling convention.
The interpreter currently refuses native foreign calls; D does not add native callback support to it.
Its owned-environment behavior remains a managed execution control.
Required witnesses include deferred fire after collection, repeated environments, release during fire, nested captures, and Context destruction.
These are estimates from code, not measured implementation line counts or passing tests.

The largest D risk is retention for the default Context lifetime, plus premature collection if either root path omits the environment.
Recommendation: consider D only with an explicit owner decision to admit captures and revise section 14.4a identity.
Use explicit registrations for repeated short-lived requests so that each environment can lose its host root.
D solves a useful host pattern but changes lifetime semantics and runtime retention; F has a smaller runtime scope.

F test impact separates a changed diagnostic from a newly accepted program.
Five existing test functions fail because of the prototype, including the corpus pin test:

| Test | Cause | Realistic host pattern |
|---|---|---|
| `receiver_capture_tests::capture_escape_has_clean_controls` | Earlier nominal rejection changes the expected constructor message | A callback reads a receiver field |
| `escaping_lambda_c_callback::a_c_callback_parameter_rejects_a_capturing_lambda` | Earlier S100 replaces the expected boundary S009 | A helper registers a callback with a captured local |
| `escaping_lambda_c_callback::c_callback_kind_dominates_managed_storage_and_propagates` | Earlier S100 replaces the expected boundary S009 | Helpers forward a callback to script storage and C |
| `corpus_reject::every_reject_entry_fails_with_its_rule_code_at_the_offending_line` | The one r502 pin changes | The same helper registration pattern |
| `apparent_type_shapes::every_value_shape_test_uses_the_apparent_type` | The prototype introduces a new raw type match and changes a source fingerprint | No host program; this is a compiler source audit |

The first four failures preserve rejection. Their programs are realistic, but F changes their diagnostic contract.
The fifth failure requires the usual source-audit update in a production implementation.
The two C callback tests do not reach their clean controls after the diagnostic assertion fails.
The separate conversion controls and accept corpus establish clean conversion acceptance.

Nine other failed test functions also fail after full restoration; they are not F regressions:

| Test | Existing failure pattern |
|---|---|
| `async_function_values::owned_async_captures_keep_borrowed_transitive_and_map_restrictions` | An async closure retains a closure that captures a local |
| `const_local_function_adapter::the_adapter_escapes_as_a_written_lambda_does` | An adapter retains a stored function value |
| `function_value_reference::field_call_is_an_indirect_capture_boundary` | An indirect call accepts a capturing callback |
| `module_global_names::symbol_bearing_error_paths_render_source_names` | An invalid-call witness is accepted |
| `nullable_function::nullable_function_capture_diagnostic_groups_the_function` | A nullable closure escapes |
| `nullable_function::nullable_capture_follows_the_storage_boundary` | A nullable closure crosses storage |
| `promise_reaction::a_direct_callback_owns_its_captures_and_a_held_lambda_does_not` | A held closure reaches a Promise reaction |
| `promise_reaction::a_rejected_capture_reports_one_diagnostic_with_its_accepted_form` | A Promise reaction captures a held closure |
| `stdlib_array_search::flat_map_cannot_return_a_capture_from_its_callback` | A collection callback returns a closure |

The closure and storage patterns are realistic script patterns; the invalid-call case is a diagnostic witness.
The prototype full-suite run also loses the `lambda_type_parameter_place` process to SIGKILL.
That process supplies no semantic result and does not count as a nominal conversion failure.
No claim of a fully passing compiler suite follows from this measurement.

| Final measurement | Result |
|---|---|
| F accept corpus | 434 accepted, zero changed entries |
| F reject pins | 458 checked, one changed diagnostic, zero new acceptances |
| F interop | 14 mirror tests and one discovery test pass |
| F full compiler run | 1,514 passed, 14 assertion failures; one test process ends with SIGKILL |
| Restored full compiler run | 1,520 passed, nine assertion failures; no lost process |
| F failures absent from restored run | Five, including the reject corpus pin test |
| Stock project tsc | Exit 0 |
| Prototype restoration | Every saved file equals its pre-prototype bytes |
| Final `git diff --check` | Pass |

The full-run totals include temporary measurement tests in F only; they are not equal test populations.
The all-entry sweeps above supply the corpus counts instead of the stop-on-first-failure corpus test.
All temporary source files and prototypes are removed. Only this tracking note changes in round 4.
The prior working-tree changes remain intact. No commit, push, gate run, or spec block edit occurs.
All shell jobs from this round end; no background task remains.


## Round 5 implementation

`FuncType.callback_typedef` holds the mirror typedef name. Equality, hashing, and debug text include this field.
A named function or a clean lambda converts at an expected callback place. Plain function values cannot cross either way.
A capture fails with S100 and the callback typedef name. The r502 pin now requires S100.
The source corpus adds r503 (field), r504 (return), r505 (indirect call), r506 (array push), and r507 (holder method).
Stock tsc accepts all five entries with the project options and mirror declarations.

### Type operation audit

| Site | Identity check or preservation |
|---|---|
| `types.rs::FuncType`, `Type::func` | Derived equality and hash include the name; fresh plain signatures use `None`. |
| `types.rs::display_type` | Callback diagnostics print the typedef name. |
| `types.rs::erase_undefined` | Visits components in place; keeps the name. |
| `check/signatures.rs::resolve_mirror_signatures` | Assigns identity to mirror function aliases after the boundary layout diagnostic. |
| `check/tyres.rs::resolve_type_ref` | Clones the mirror alias; keeps identity. |
| `check/source_alias.rs`, `check/lookup.rs` | Resolved source aliases clone the type; aliases keep identity. |
| `check/type_rules.rs::assignable_through_constraints` | Compares callback identity before function variance. |
| `check/type_rules.rs::function_fits`, `type_fits`, `function_of` | Compare identity or clone the full signature, including nullable forms. |
| `check/type_rules.rs::place_value` | Converts clean literal or named-function values; blocks callback-to-plain adapters. |
| `check/type_rules.rs::pad_lambda_parameters`, `discard_lambda_result`, `adapt_function_reference` | Fresh adapters are plain; nominal place checks prevent identity loss. |
| Array, Map, FixedArray assignment and argument checks | Container equality includes the nested callback identity. Expected elements use `place_value`. |
| `check/generics.rs::numeric_normal_form` | Copies the typedef name when it rebuilds a signature. |
| Generic resolution and instance type arguments | Resolve mirror references with their identity; instance keys use complete types. |
| `check/expr/operator.rs::check_as` | Concrete function casts stay rejected. Generic casts cannot create or erase callback identity. |
| `check/type_rules.rs::contains_callback_identity` | Checks nested components and generic class arguments before a generic cast. |
| `check/opaque.rs::generic_overlap` | Function overlap uses assignability; the cast also checks callback identity. |
| `codegen/src/reload.rs::signature_text`, `declaration_hash` | Debug type text includes the typedef name; a test compares independently checked signatures. |
| `check/signatures.rs` boundary functions, `check/class_shape.rs` fields and constructors | Keep resolved semantic types and their identity; C typedef metadata stays separate. |
| `hir/parameter.rs`, LIR boundary lowering, C emission | Clone complete checked types; identity does not change the function representation. |
| `check/interface.rs` and synthetic codegen signatures | Construct fresh plain function signatures with `None`; no mirror type is rebuilt here. |

Checker tests reject identity loss through aliases, nullable types, arrays, Map, FixedArray, generic fields and arguments, and casts.
Tests also compare distinct callback typedefs with the same signature and reject a generic cast from a plain function.
The reload test distinguishes two callback typedefs and a plain function signature.

`parameter_capture::accepts_owned` now reads HIR ownership flags and has no unused argument.
Its cases check push, yield, a held async argument, a loop back edge, a passed-on parameter, and an indirect call.
Old rejection assertions for managed captures now require acceptance under compiler.md §274.
`a447` contains the source shapes of r10 and r240 through r248, with separate names.
Its hand-computed golden is `10` then eight lines of `5`. The three tiers and Node agree.
`a446` retains its golden and its memory accounting check.
The debug-profile interop tests run ship C callbacks without the runtime null-environment assertion.

### Files in the cumulative working tree

The inherited changes include the collision contract. This round does not edit a spec block.
No commit or push occurs.

- `codegen/src/cemit.rs`
- `codegen/src/layout.rs`
- `codegen/src/lib.rs`
- `codegen/src/lir.rs`
- `codegen/src/reload.rs`
- `codegen/tests/escaping_lambda_environment.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/capture/blocks.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/expr/promise_reaction/helper.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/interface.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/receiver_capture_tests.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/diag.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/parameter.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/tests/collections.rs`
- `compiler/src/types.rs`
- `compiler/src/types/absence_tests.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/async_function_values.rs`
- `compiler/tests/const_local_function_adapter.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/counted_capture_block.rs`
- `compiler/tests/escaping_lambda_c_callback.rs`
- `compiler/tests/escaping_lambda_environment.rs`
- `compiler/tests/function_value_reference.rs`
- `compiler/tests/module_global_names.rs`
- `compiler/tests/nullable_function.rs`
- `compiler/tests/parameter_capture.rs`
- `compiler/tests/promise_reaction.rs`
- `compiler/tests/stdlib_array_search.rs`
- `corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.expected`
- `corpus/accept/a446-a-capturing-lambda-that-escapes-owns-its-environment.ts`
- `corpus/accept/a447-retired-capture-escape-shapes.expected`
- `corpus/accept/a447-retired-capture-escape-shapes.ts`
- `corpus/reject/r10-escaping-capture.ts`
- `corpus/reject/r240-parameter-stored-in-field.ts`
- `corpus/reject/r241-parameter-returned.ts`
- `corpus/reject/r242-expression-body-returns-parameter.ts`
- `corpus/reject/r243-capture-passed-to-generator.ts`
- `corpus/reject/r244-capture-passed-to-async.ts`
- `corpus/reject/r245-yield-returns-capture.ts`
- `corpus/reject/r246-capture-through-loop-back-edge.ts`
- `corpus/reject/r247-generator-parameter-stored.ts`
- `corpus/reject/r248-capture-through-indirect-call.ts`
- `corpus/reject/r502-a-capturing-lambda-is-not-a-c-callback.ts`
- `corpus/reject/r503-c-callback-through-field.ts`
- `corpus/reject/r504-c-callback-through-return.ts`
- `corpus/reject/r505-c-callback-through-indirect.ts`
- `corpus/reject/r506-c-callback-through-array.ts`
- `corpus/reject/r507-c-callback-through-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md`
- `specs/tracking/escaping-lambda-environment.md`

### Final result lines

Times below are measured wall times, including Cargo startup.

| Check | Result | Seconds |
|---|---|---:|
| Compiler library unit tests and generated references | 612 passed | 5.561 |
| Callback identity and apparent-type audit | 13 passed | 3.528 |
| Corpus accept/reject and related checker integration tests | 156 passed | 16.358 |
| a446/a447 three-tier, memory, reload identity, and debug interop | 25 passed | 8.111 |
| Project tsc | Exit 0 | 0.680 |
| r503–r507 tsc with project options and mirrors | Exit 0 | 0.199 |
| a446/a447 Node, byte comparison with goldens | Pass | 0.148 |
| Clippy, compiler and codegen, all targets | Exit 0; no warnings in changed files | 7.156 |
| cargo fmt --check | Exit 0 | 2.820 |
| git diff --check | Exit 0 | 0.040 |

The broad compiler integration sweeps exposed old managed-capture rejection assertions; the related tests now require acceptance.
Two broad sweeps were interrupted after the required suites completed separately; neither counts as a full integration pass.
The final library suite and listed integration suites pass. No gate script runs.
All shell jobs started by this round have ended or were interrupted. No background task remains.


## Round 6 stop: duplicate source positions in the execution fact check

The a446 lambda at line 13, column 18 has two generic instances.
Their captures are `dd: ClassId(1)` (A) and `dd: ClassId(2)` (B).
Temporary lowering instrumentation confirms that both instances have `owned_environment = true`.
Each instance gets its own capture class and `OwnedEnvironment` parameter through the existing lowering.
The instrumentation is removed.

`codegen/tests/support/lir_facts/async_environments.rs` selects a lambda with `find` by kind and source position only.
Both instances have the same source position, so the check selects the first instance for both HIR expressions.
Its field-type comparison cannot match both distinct capture types.
A lowering change cannot satisfy this check while it preserves both capture types and source positions.
The handoff prohibits a fact-check change and requires a stop if the form cannot carry the required fact.
No production code, fact check, or snapshot changes in this round.
The remaining repairs and checks stay pending.

| Check | Result | Wall seconds |
|---|---|---:|
| LIR execution fact test, debug | Failed: one a446 finding | 7.62 |
| Full `codegen/tests/lir.rs`, debug | 65 passed, 2 failed | 13.60 |

The second full-suite failure is the LIR text snapshot at line 409.
No snapshot capture or replacement occurs after the step 1 stop.
The full LIR log is `/tmp/274-lir-full.log`.

Open item: a clean conditional `flag ? f : g` fails at a callback place.
`place_value` converts only a direct lambda or function name.


## Round 7: instance key and snapshot stop

The owning-environment check now selects a lambda by its enclosing function instance and source position.
Declaration order identifies the enclosing instance; MakeClosure edges identify its nested lambda functions.
All layout, capture type, capture name, and environment parameter comparisons stay unchanged.
The a446 control reports the line 13 column 18 mismatch with the position-only key.
The same control reports no finding with the instance key.

The snapshot capture differs in 37 entries. The committed snapshot stays unchanged.
Several entries change only because derived Debug now prints `callback_typedef: None` for plain function types.
This is neither an owned environment change nor callback typedef text.
Handoff 274-6 item 2 requires a stop for this difference.
The full diff is `/tmp/274-snapshot.diff`; the capture is the tool output `subscript-lir-goldens.txt` in TMPDIR.

| Entry | Reason |
|---|---|
| `a111-interop-async-method-poll` | Callback typedef identity text (rule 7); also plain function type text where present. |
| `a128-host-owned-state` | Callback typedef identity text (rule 7); also plain function type text where present. |
| `a146-scoped-locals` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a149-suspension-state` | Callback typedef identity text (rule 7); also plain function type text where present. |
| `a151-lambda-env-outlives-block` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a152-lambda-env-per-iteration` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a171-static-array-callbacks` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a261-lambda-borrows-a-captured-handle` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a329-initializer-inference` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a330-this-capture` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a336-async-function-values` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a342-awaited-nested-closure` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a345-counted-capture-block` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a347-host-operation-completes` | Callback typedef identity text (rule 7); also plain function type text where present. |
| `a353-async-arrow-captures` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a354-completion-string-bytes` | Callback typedef identity text (rule 7); also plain function type text where present. |
| `a356-file-completion-work` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a357-promise-then-chains` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a358-promise-catch-finally` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a359-promise-then-captures` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a360-class-then-method` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a361-promise-then-order` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a392-static-method-and-local-function-values` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a406-fixed-array-traversal-storage` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a410-reduce-seed-await` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a420-then-result-takes-context` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a423-async-return-adopts-a-handle` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a424-void-function-returns-a-void-operand` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a425-lambda-with-no-value-result-is-void` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a426-recursive-void-lambda` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a427-named-function-takes-a-void-result-place` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a431-nested-then-takes-the-context-of-its-adoption` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a432-a-const-local-function-value-takes-an-adapter-that-captures-it` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a435-a-void-function-returns-undefined` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a439-an-operand-hold-is-a-release-owner` | Only callback_typedef: None text; no callback typedef or owned environment change (outside permission). |
| `a447-retired-capture-escape-shapes` | New async-main entry (allowed). |
| `a95-interop-async-await` | Callback typedef identity text (rule 7); also plain function type text where present. |

Prepared repairs: place_value lists both type-only calls and their reasons; the owns_environment comment cites rule 1.
The C callback diagnostic states its cause and type name.
Reject headers r503 through r507 pin their line; r504 has no unused main-local constant.
These repairs remain unverified after the snapshot stop.

Files changed this round: codegen/tests/lir.rs, codegen/tests/support/lir_facts.rs,
codegen/tests/support/lir_facts/async_environments.rs, compiler/tests/type_only_assignability.rs,
compiler/src/hir.rs, compiler/src/check/type_rules.rs, corpus/reject/r503 through r507,
and this tracking note.

| Check | Result | Wall seconds |
|---|---|---:|
| Owning generic lambda key control | 1 passed | 0.11 |
| LIR snapshot capture to a copy | 1 passed (capture mode; no comparison) | 4.39 |
| cargo fmt --all | Passed | 3.52 |

Full LIR, type-only, corpus, remaining new tests, fmt --check, and clippy stay pending at the required stop.
Open item: a clean conditional `flag ? f : g` fails at a callback place.
`place_value` converts only a direct lambda or function name.

## Round 8: stable function text and snapshot

`FuncType` has a custom Debug implementation. A plain function keeps its text at the contract pin.
A callback type prints `callback_typedef` with its name only when the identity is present.
LIR, HIR, nested type text, and reload declaration hashes share this implementation.
The plain type contributes the same text to the declaration hash as at the pin.
Callback names contribute distinct text. Other declaration facts can still change a hash.
The Debug text test checks both forms. The reload test checks distinct callback identities and the plain type.

The capture tool wrote a copy. A diff of that copy permits these seven entries only.
The captured copy replaces the snapshot. The temporary snapshot diff is removed.

| Entry | Reason |
|---|---|
| `a111-interop-async-method-poll` | Rule 7: callback fields name `SubLogCallback`. |
| `a128-host-owned-state` | Rule 7: callback fields name `SubLogCallback`. |
| `a149-suspension-state` | Rule 7: callback fields name `SubLogCallback`. |
| `a347-host-operation-completes` | Rule 7: callback fields and closure value types name `SubLogCallback`. |
| `a354-completion-string-bytes` | Rule 7: callback fields and closure value types name `SubLogCallback`. |
| `a447-retired-capture-escape-shapes` | New entry with async main. |
| `a95-interop-async-await` | Rule 7: callback fields name `SubLogCallback`. |

No existing entry changes its environment lowering in this snapshot.
`a446` has no async function, so the snapshot selector excludes it.
The type-only test passes with two listed calls in `place_value`.
The owns_environment comment cites rule 1. The callback diagnostic names its cause and target type.
Headers r503 through r507 pin their line. r504 has no unused local constant.
The callback identity test now expects `Other` when `Other` is the destination typedef.
The API reference generator updates the generated references after the initial byte comparison failure.

| Check | Result | Seconds |
|---|---|---:|
| LIR snapshot capture | 1 passed; test time, build 9.14 seconds | 4.32 |
| Full LIR suite | 68 passed; wall time | 12.94 |
| Initial compiler group | Failed: generated reference byte comparison; wall time | 10.52 |
| Compiler library after generation | 612 passed | 2.16 |
| Accept corpus | 13 passed | 3.06 |
| Reject corpus | 48 passed, 1 failed: missing divergence blocks | 1.13 |
| Warn corpus | 3 passed | 3.11 |
| Initial callback tests | 5 passed, 1 failed: expected Callback instead of Other | 0.07 |
| Final callback tests, including Debug control | 6 passed | 0.07 |
| Compiler environment tests | 2 passed | 0.02 |
| Node corpus and collision checks | 11 passed | 0.59 |
| TypeScript corpus | 11 passed | 2.07 |
| Type-only assignability | 6 passed | 0.12 |
| Codegen environment, three tiers and reload hash | 4 passed | 0.54 |
| Reload suite | 37 passed | 8.32 |
| Initial fmt check | Failed: new Debug control needed formatting; wall time | 2.03 |
| Final fmt check | Passed; wall time | 2.02 |
| Final workspace clippy, all targets | Passed with warnings outside changed files; wall time | 2.54 |

Files changed in this round: compiler/src/types.rs, compiler/tests/escaping_lambda_c_callback.rs,
codegen/tests/lir-goldens/corpus.txt, generated-docs/language-reference.md, and this tracking note.
The generator also writes generated-docs/corpus-index.md; its contents stay as in round 7.
Prepared round 7 repairs remain in the working tree.

Open failure: `divergence_blocks_match_every_reject_entry_tsc_header` reports r502 through r507.
The callback refusal uses `AssignmentTypeMismatch`, which classifies the rejection as `TscRejects`.
The header and measured TypeScript results say TypeScript accepts these programs.
No existing rejection class describes rule 7 callback identity. The diagnostic needs a distinct divergence class and witness.
The current result does not complete the phase.

Open item: a clean conditional `flag ? f : g` fails at a callback place.
`place_value` converts only a direct lambda or function name.
No background task remains from this round.
