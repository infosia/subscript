# §167 async function values

## Return rule

Rule 3 at `5ea5e619` treats an async expression body as a named async return.
The checker does not convert a handle body to `AsyncHandleAwait`.
A handle body gives S100: "the return value expects `i32`, got `Promise<i32>`".
The diagnostic carries `AsyncReturnHandle`, with `compiler.md §167` as its decision.
An explicit `await h` supplies the fulfilled value and passes.

TypeScript 5.9.2 accepts the handle body, the named return, and the explicit-await control.
Node v24.18.0 runs the control's CommonJS emit with `print` bound to `console.log`.
The control prints `result 7` after `turn 1` and before `turn 2`.
The interpreter, JIT, and C AOT match that output.

`handle_expression_body_rejects_with_explicit_await_order_control` pins both results.
`handle_returns_reject_with_explicit_await_controls` covers annotations, contextual types, inference, block bodies, and named functions.
Each rejected body has an accepted explicit-await control.
`r382-async-arrow-handle-body` pins the expression-body rejection in the corpus.
Its TypeScript 5.9.2 project exits 0.
An isolated CLI build from `8cea0f9f` exits 1 with `AsyncArrowFunction` at 9:15.
That production source matches `5ea5e619`; the final checker instead rejects the return value at 9:56.

## Corpus evidence

The Red evidence uses `c7afd7b68b3a5d8f84f27e5913a017f14f952b77`.
The amended contract pin is `8cea0f9f`; its only change adds rule 13.
The CLI build used `cargo build --offline --locked -p subscript-cli` at that pin.
Each CLI input was a copy outside the repository.

TypeScript 5.9.2 used one temporary project per entry.
Each project extended `tsconfig.json` and included `prelude/**/*.d.ts`.
The emit used CommonJS and `moduleResolution: node10`.
Node v24.18.0 used the emitted JavaScript, with `print` bound to `console.log`.

| Entry | TypeScript exit | Pin CLI exit | Pin result |
|---|---:|---:|---|
| `a336-async-function-values` | 0 | 1 | 14 diagnostics; the first is S100 `AsyncFunctionValue` at 14:45 |
| `r379-async-arrow-capture` | 0 | 1 | S100 `AsyncArrowFunction` at 10:15; the contract requires S009 |
| `r380-dropped-indirect-async-handle` | 0 | 1 | S100 `AsyncFunctionValue` at 10:41, then S013 at 11:3 |
| `r381-async-arrow-result-annotation` | 2 | 1 | S100 `AsyncArrowFunction` at 9:15; TypeScript reports TS1064 at 9:25 |

Node exits 0 for `a336`.
Its 29 output lines match `a336-async-function-values.expected` on a second run.
The entry covers every shape in §167.2 acceptance 1 and adds a contextual block-body arrow.
No existing `.expected` file changes.

The reject test requires S009 at line 10 for `r379`, S013 at line 11 for `r380`, and S100 at line 9 for `r381`.
`every_reject_entry_fails_with_its_rule_code_at_the_offending_line` fails at `r379`: the pin checker gives S100 instead of S009.
`cargo fmt --check` and `git diff --check` exit 0.
`reject_table_covers_every_corpus_entry` passes.

## Callable form

The HIR lambda carries `is_async` and its body result in `ret`.
The expression type carries the callable result, `Promise<T>`.
Deferred initializers retain both facts.
The layout check selects a coroutine frame for an async body.

A synchronous arrow that returns a handle keeps its synchronous body.
An async expression body follows the named async return check.
The async control uses an explicit await; the synchronous control returns the handle.
Both controls print `7` in all three tiers.

Each async callable creates a frame, starts its body, and returns a handle with one owner.
The JIT and C emitter use callable wrappers.
The interpreter starts the coroutine in `invoke_callable`.
Each consumer uses the ordinary indirect `Call`.
An immediate await transfers the fresh owner through `SuspendKind::AsyncHandle.owned`.

A named callable resolves its creator through the JIT function table after reload.
An arrow callable retains its module body.
The existing reload epoch check rejects a suspended arrow frame with `StaleCoroutine`.
The runtime requires no change: its register, resume, completion, ownership, and reload operations serve both producers.

## Rejection sites

`AsyncFunctionValue` and `AwaitLocalCall` leave the site table.
`AsyncArrowFunction` rejects only generic async arrows.
`AwaitIndirectCall` rejects a call that returns no handle.
Async method values and generator arrows retain their restrictions.

| Site | Witness | TypeScript 5.9.2 | Class |
|---|---|---|---|
| `AsyncArrowCapture` | `s167-capture`, `r379` | accepts | `Diverges`, C24 row 36 |
| `AsyncArrowResultAnnotation` | `s167-result`, `r381` | rejects TS1064 | `TscRejects` |
| `AsyncArrowFunction` | `r-s167-generic` | accepts | `Diverges`, C8 |
| `AsyncReturnHandle` | `s167-return-handle-arrow`, `s167-return-handle-named`, `r382` | accepts | `Diverges`, compiler.md §167 |

The capture diagnostic names the local, parameter, or `this` binding.
The completed-module capture check also rejects deferred capture environments.

## Unit evidence

`compiler/tests/async_function_values.rs` checks deferred body facts, captured bindings, and completion observation.
Each check has a same-shape accepted control.
Nested lexical receiver captures give S009 through one or two synchronous arrows.
The same nested arrows pass in a synchronous outer arrow.
The capture boundary check supplies this rejection before the receiver reaches lowering.

`codegen/tests/async_function_values.rs` compares `a336` with its Node golden in all three tiers.
It also checks a synchronous handle arrow against an async arrow with the same annotation and expression body.

The ownership test creates twenty handles and compares the final allocation count with the count before the root starts.
A module-global Error supplies the failure object, so the test measures frames without new Error allocations.
Success and failure restore the initial count without collection.
Direct calls supply the controls for indirect calls.

An async arrow in an instance field returns `7` after explicit collection in all three tiers.
The control omits collection.
A suspended arrow traps `StaleCoroutine` after reload; the control finishes with `7`.
A named async value returns `9` after a body edit; the control returns `7`.
A try block inside an async arrow catches an exception after an await and returns `7`.
The same named async body supplies its control.

## Async cost

The baseline is `8cea0f9f`.
The measurement uses the existing `async-cost` release binary, its default warm-up, and eleven timed samples per run.
Each revision has three runs.
The selected median is the smallest per-run median.
The benchmark runs alone, without another benchmark or build.
Times below use nanoseconds.

| Workload | Pin medians | Tree medians | Pin selected | Tree selected | Ratio |
|---|---|---|---:|---:|---:|
| settled-awaits | 36107000, 36732000, 36075000 | 32963000, 33574000, 33435000 | 36075000 | 32963000 | 0.9137 |
| held-handles | 12362000, 12258000, 12439000 | 11299000, 11399000, 11397000 | 12258000 | 11299000 | 0.9218 |
| deep-chains | 20618000, 21284000, 21344000 | 19100000, 19189000, 19426000 | 20618000 | 19100000 | 0.9264 |

Each measured workload stays below the 1.05 limit.
The tree measurements use the final release binary with the rule 3 return check.
All three runs finish without another build, test, or benchmark.

## Validation

`cargo build --offline --locked --workspace --all-targets` passes.
The full compiler, codegen, and runtime test suites pass with `--offline --locked`.
The focused compiler suite passes five tests; the focused codegen suite passes nine tests.
The §154 total test passes with the named and arrow handle-return witnesses.
The reject registry and the TypeScript-header divergence check pass with `r382`.

`cargo fmt --check` and `git diff --check` pass.
Workspace all-target clippy passes.
A comparison with the pin finds no new warning message and source-file pair.
The pin has 39 distinct pairs; the tree has 38.
The removed warning reports identical blocks in the await operand checker.
`tools/hygiene.sh` passes.
Each changed Rust file has at most 2,000 lines.
No existing `.expected` golden changes.

## Retired entry and citations

Retired corpus: `retired:r140`.
`r140-async-lambda.ts` retires.
The reject registry and Q34 corpus list no longer reference it.
The corpus index reads the explicit retirement record from this note.
It still rejects a retired entry that exists, or a collision rule without live corpus evidence.

These specification references remain intact:

| File | Lines |
|---|---|
| `specs/tracking/r36-async-generics.md` | 44, 48, 70 |
| `specs/blocks/compiler/s064-r36-async-methods-on-generic-classes-generic-async-functions.md` | 92, 111 |
| `specs/blocks/compiler/s167-async-function-values-and-non-capturing-async-arrows.md` | 18, 115 |

C8 cites `a336` and `r380`–`r382`, and marks `retired:r140-async-lambda` in the §64 and §167 revisions.
C24 row 36 cites `r379` and `a336`.

## Documentation

Q34 states the async function value, the non-capturing async arrow, and the accepted indirect await operands.
Its corpus list includes `a336`, `r379`, `r380`, `r381`, and `r382`.
Q34 also states the expression-body return rule and its explicit-await form.
The tutorial states §167 rules 2 and 5.

The generator changes `generated-docs/corpus-index.md` and `generated-docs/language-reference.md`.
`generated-docs/api-reference.md` has identical bytes.
The LIR text golden adds `a336`.
No existing `.expected` file changes.

## Changed files

| File | Change |
|---|---|
| `codegen/src/cemit.rs` | Modified |
| `codegen/src/cemit/async_callable.rs` | Added |
| `codegen/src/cemit/call.rs` | Modified |
| `codegen/src/cemit/emitter.rs` | Modified |
| `codegen/src/cemit/literal.rs` | Modified |
| `codegen/src/interpreter.rs` | Modified |
| `codegen/src/lir/lambda.rs` | Modified |
| `codegen/src/lower/func.rs` | Modified |
| `codegen/src/lower/func/async_callable.rs` | Added |
| `codegen/src/lower/func/builtin.rs` | Modified |
| `codegen/src/lower/func/call.rs` | Modified |
| `codegen/src/lower/mod.rs` | Modified |
| `codegen/tests/async_function_values.rs` | Added |
| `codegen/tests/lir-goldens/corpus.txt` | Modified |
| `codegen/tests/support/lir_facts.rs` | Modified |
| `compiler/src/check/capture.rs` | Modified |
| `compiler/src/check/expr/entry.rs` | Modified |
| `compiler/src/check/expr/lambda.rs` | Modified |
| `compiler/src/check/expr/literal.rs` | Modified |
| `compiler/src/check/layout.rs` | Modified |
| `compiler/src/check/lookup.rs` | Modified |
| `compiler/src/check/stmt.rs` | Modified |
| `compiler/src/check/type_rules.rs` | Modified |
| `compiler/src/check/rejection.rs` | Modified |
| `compiler/src/check/rejection_programs.txt` | Modified |
| `compiler/src/check/rejection_sites.rs` | Modified |
| `compiler/src/check/rejection_targets.txt` | Modified |
| `compiler/src/check/rejection_witness_index.rs` | Modified |
| `compiler/src/check/rejection_witness_sites.rs` | Modified |
| `compiler/src/divergence.rs` | Modified |
| `compiler/src/divergence/builtin_calls.rs` | Modified |
| `compiler/src/divergence/entries.rs` | Modified |
| `compiler/src/divergence/established_tail.rs` | Modified |
| `compiler/src/divergence/surface_forms.rs` | Modified |
| `compiler/src/hir.rs` | Modified |
| `compiler/src/hir/tests.rs` | Modified |
| `compiler/src/language_reference.rs` | Modified |
| `compiler/tests/async_function_values.rs` | Added |
| `compiler/tests/corpus_reject.rs` | Modified |
| `compiler/tests/js_corpus.rs` | Modified |
| `corpus/accept/a336-async-function-values.expected` | Added |
| `corpus/accept/a336-async-function-values.ts` | Added |
| `corpus/reject/r140-async-lambda.ts` | Deleted |
| `corpus/reject/r379-async-arrow-capture.ts` | Added |
| `corpus/reject/r380-dropped-indirect-async-handle.ts` | Added |
| `corpus/reject/r381-async-arrow-result-annotation.ts` | Added |
| `corpus/reject/r382-async-arrow-handle-body.ts` | Added |
| `docs/tutorial-typescript.md` | Modified |
| `generated-docs/corpus-index.md` | Modified |
| `generated-docs/language-reference.md` | Modified |
| `specs/tracking/s167-async-function-values.md` | Added |

## Phase Review and gate

The Phase Review finds no CRITICAL or MAJOR issue.
Its five MINOR findings are §167.3 items 1 to 5.
It compares `a336` and fifteen probes in the dev JIT, C AOT, and `node`, byte for byte.
The full gate at `5ea5e619` reports debug 2402/0/3, release 2399/0/3, exit 0.
Its one moved golden is the LIR text golden: the `a336` functions.
