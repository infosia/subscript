# Local lambda self-call

Contract: compiler.md §284.
Pin: `78984dbc5bdc0d86c54e061755ca12401901a2e7`.

## Design

The HIR self-call must carry the lambda identity and its decided function signature.
It must refer to the current environment without a source-name lookup.
The checker must exclude this call from capture and escape facts.
The LIR self-call must carry the target function and the current environment operands.
The verifier must compare these operands with the target function parameters and the caller identity.
Each execution tier must pass the current environment without a new environment allocation.
An async self-call must return the ordinary async handle.
The environment mode must come from the lambda form, including the ownership fact of §274.
The implementation below follows this design.

## Measurements

The initial work tree was clean.
The CLI build used unchanged sources at the pin.
A separate copy of that binary measured each Red case before any compiler change.

| Command or case | Result | Wall time |
|---|---|---:|
| `cargo build --offline -p subscript-cli` | PASS; pin binary | 0.14 s |
| Pin CLI, `check a449` | RED; six S100 diagnostics | 0.91 s |
| Pin CLI, module-level control | PASS; all six module forms run | 0.05 s |
| `tsc -p tsconfig.json` | PASS; includes a449 | 0.66 s |
| `tsc`, r508 | ACCEPTS | 0.178 s |
| `tsc`, r509 | ACCEPTS | 0.178 s |
| `tsc`, r510 | ACCEPTS | 0.178 s |
| Node, a449 | PASS; exit 0, 40 output lines | 0.032 s |
| Pin CLI, r508 | S100 at 8:65 | <0.01 s |
| Pin CLI, r509 | S100 at 8:51 | <0.01 s |
| Pin CLI, r510 | S100 at 8:48 | <0.01 s |
| `cargo fmt --check` | PASS | 2.02 s |

The a449 diagnostics name `escaped`, `annotated`, `inferred`, `captured`, `asynchronous`, and `loop`.
Each diagnostic says that the name is read before its declaration in this block.
The module-level control uses the six module declarations from a449 and calls each declaration.
The measurements used TypeScript 5.9.2 and Node v24.22.1.
Node output is a measurement only. No golden was written from Node output.

## Environment operands

The LIR function of a lambda holds its environment as parameters.
An owning lambda has one `OwnedEnvironment` parameter (§274).
A borrowed lambda has one `Capture` parameter for each capture.
A self-call passes these parameters, in parameter order, as its environment operands.
Each tier converts them to its lambda ABI, as it does for a known closure target.

## Implementation

The checker marks the call as `Callee::LambdaSelf { id, signature, is_async }`.
The binding applies only to the whole local `const` initializer and the current lambda body.
Parentheses keep the whole initializer form.
A shadow declaration or a nested function boundary stops this binding.
The callee adds no capture and no escape.
The capture analysis follows explicit arguments through the current lambda parameters.
The async fact distinguishes an async lambda from a synchronous lambda that returns a handle.

The LIR call kind is `CallTargetKind::LambdaSelf(FunctionId)`.
Its operands are the current `OwnedEnvironment` parameter, or the current `Capture` parameters, followed by explicit arguments.
The verifier compares argument types and the result with the target declaration.
It also checks the caller identity and the environment parameter origins.
An ordinary `Function` call cannot target a lambda.
The verifier test changes executable operands and targets; it does not change the recorded signature.

The interpreter passes these operands and starts an async handle through the ordinary callable path.
The development tier packs borrowed captures into stack ABI storage and passes an owned environment directly.
It shares the call ABI conversion with `static_closure_call`.
The ship tier emits the same conversion and the same async wrapper call.
A self-call allocates no heap environment.
An async self-call creates and starts its ordinary result handle.

The checker unit test checks an empty capture list and false environment ownership.
It checks an annotated body, a syntax-decided void body, and a parenthesized initializer.
The verifier test rejects a different target, a different same-type environment operand, a wrong argument type, and a missing call kind.
The extra runtime test checks aggregate captures and results, function captures, owned environments, and a self-call after an await.
It also checks a synchronous self-call that returns a Promise.
All three tiers match the hand-stated output of that test.

The reject corpus test registry adds r508, r509, and r510.
That registry must name every reject entry and its diagnostic line.
No rejection-site registry changes, and no entry moves.
The document generator changes only `generated-docs/corpus-index.md`.
The snapshot tool adds 641 lines for a449 to `codegen/tests/lir-goldens/corpus.txt`.
A block comparison checks that every existing snapshot entry stays byte-identical.
The only new output golden is the 40-line a449 capture.
Node and both native CLI runs equal this golden.
The full debug LIR interpreter sweep also equals the golden.

## Measurements after the change

All commands use the pinned debug toolchain and offline Cargo dependencies.
Some commands run concurrently and wait for the shared Cargo build directory.
The command wall times include those waits.
The test runner times below are the durations that Cargo prints for each result line.

| Command or case | Result | Wall time |
|---|---|---:|
| Initial compiler library check, before golden capture | 612 PASS; generated-reference test fails because a449 has no output file | 9.30 s |
| Initial reject corpus check | PASS; 49 tests; r508 to r510 keep S100 | 7.77 s |
| LIR check after all tier paths compile, before golden capture | 66 PASS; two failures require the a449 golden and snapshot | 12.48 s |
| `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1 cargo test --offline -p subscript-codegen --test lir coroutine_and_measurement_lir_text_matches_goldens` | PASS; 1 test | 8.47 s |
| `cargo run --offline -p subscript-compiler --bin generate-api-reference` | PASS; generator writes the three documents | 3.02 s |
| `cargo build --offline -p subscript-cli --bin subscript -p subscript-codegen --bin capture` | PASS | 0.94 s |
| Debug CLI, a449 dev run | PASS; equals golden | 0.86 s |
| Capture tool, a449 | PASS; equals golden | 0.79 s |
| Debug CLI, a449 ship build and run | PASS; equals golden | 1.23 s |
| Node corpus runner, a449 | PASS; equals golden | 0.41 s |
| `tsc -p tsconfig.json` | PASS | 3.27 s |
| Debug CLI, r508 check | S100; expected exit 1 | 0.006 s |
| Debug CLI, r509 check | S100; expected exit 1 | 0.005 s |
| Debug CLI, r510 check | S100; expected exit 1 | 0.005 s |
| `cargo test --offline -p subscript-compiler --lib --test corpus_accept --test corpus_reject --test js_corpus` | PASS; all four test targets | 27.95 s |
| `cargo test --offline -p subscript-codegen --lib --test lir --test local_lambda_self_call` | PASS; all three test targets | 98.87 s |
| `cargo test --offline -p subscript-codegen --test golden` | PASS; 3 tests | 87.13 s |
| `cargo test --offline -p subscript-codegen --test cemit` | PASS; 106 tests | 30.50 s |
| `cargo test --offline -p subscript-compiler --lib check::rejection_total` | PASS; 19 tests | 2.18 s |
| `cargo fmt --check` | PASS | 11.97 s |
| `cargo clippy --offline -p subscript-compiler -p subscript-codegen --all-targets` | PASS; warnings in existing code | 71.82 s |
| `tools/hygiene.sh` | PASS | 3.48 s |
| Additional clippy run with `-- -D warnings` | FAIL; warnings in existing runtime, compiler functions, and tests become errors | 6.12 s |

The stricter clippy run reports existing argument-count, borrow, collection, and style warnings.
No new self-call function or test has a clippy warning.
The required clippy command completes with exit 0.

| Final test result line | Test runner wall time |
|---|---:|
| Compiler library: 613 passed; 0 failed | 2.57 s |
| Accept corpus: 13 passed; 0 failed | 10.02 s |
| Reject corpus: 49 passed; 0 failed | 4.25 s |
| JS corpus: 11 passed; 0 failed | 1.99 s |
| Codegen library: 355 passed; 0 failed | 57.31 s |
| LIR: 69 passed; 0 failed | 9.10 s |
| Extra self-call runtime: 1 passed; 0 failed | 0.32 s |
| Golden sweep: 3 passed; 0 failed | 55.74 s |
| C emission sweep: 106 passed; 0 failed | 18.19 s |
| Rejection total: 19 passed; 0 failed; 594 filtered out | 2.03 s |

## Changed files

- `codegen/src/cemit/call.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/dispatch.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/address_taken.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/defaults.rs`
- `codegen/src/lir/lambda.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_instruction.rs`
- `codegen/src/lir/verify_lifetime.rs`
- `codegen/src/lir/verify_raise.rs`
- `codegen/src/lower/func/call.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/lir/local_lambda_self_call.rs`
- `codegen/tests/local_lambda_self_call.rs`
- `codegen/tests/support/lir_facts.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/element_receiver.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/self_call.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/effects.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/lir.rs`
- `compiler/src/raise_sites.rs`
- `compiler/src/tests/local_lambda_self_call.rs`
- `compiler/src/tests/mod.rs`
- `compiler/tests/corpus_reject.rs`
- `corpus/accept/a449-a-local-lambda-calls-itself.expected`
- `corpus/accept/a449-a-local-lambda-calls-itself.ts`
- `corpus/reject/r508-nested-lambda-calls-outer-name.ts`
- `corpus/reject/r509-local-lambda-self-value.ts`
- `corpus/reject/r510-mutually-recursive-local-lambdas.ts`
- `generated-docs/corpus-index.md`
- `specs/tracking/local-lambda-self-call.md`

## Result

COMPLETE for the implementation and the requested checks.
The initial missing-golden and missing-snapshot failures are resolved.
All required test targets and the required clippy command pass.
The additional strict clippy run fails on warnings in existing code, as recorded above.
Every changed Rust file stays below 2,000 lines.
No existing output golden changes, and no entry moves.
No contract block changes.
The full gate result is below.


```text
gate full 78984dbc5bdc0d86c54e061755ca12401901a2e7 dirty:46 debug 3396/0/5 release 3377/0/5 skips 1/0 clippy 3/10/13 goldens-moved 1 exit 0
```

The moved golden is the LIR text snapshot. It grows by the functions of
`a449` only; no other entry of the snapshot changes.


## Section 285: self-call type from the const annotation

Contract: compiler.md §285, with the unchanged self-call rules of §284.
Pin: `4a1db273885b4068b33f59c0e5c672d05f049b78`.
The initial work tree was clean.
The pin CLI build preceded every compiler source change.
A separate copy of that binary measured the Red entry and the invalid-result controls.

### Red evidence

The a450 check exits 1 with these diagnostics:

```text
error[S100]: `fact` is read before its declaration in this block
  a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.ts:11:16
error[S100]: `expression` is read before its declaration in this block
  a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.ts:13:63
error[S100]: `asynchronous` is read before its declaration in this block
  a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.ts:16:22
error[S100]: `text` is read before its declaration in this block
  a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.ts:18:68
error: 4 error(s)
```

TypeScript 5.9.2 accepts a450. Node v24.22.1 gives this output:

```text
120
120
120
step:step:step:end
```

The three factorial results are each `5 * 4 * 3 * 2 * 1`.
The string result adds three `step:` prefixes to `end`.
The capture tool writes the golden from the development tier.
Node output equals the golden; Node does not supply the golden.

### Implementation and checks

The existing lambda result decision takes the contextual annotation before the body check.
The self-call admission now counts that decided contextual result.
A result hint alone does not admit a self-call.
The signature uses the existing parameter types and result type.
The span check still limits the binding to the whole local const initializer.
The existing lookup still rejects a nested-lambda capture, a value read, and mutual recursion.
The reject corpus keeps its diagnostics, including r508, r509, and r510.

The unit test compares each invalid result with its control without a self-call.
It covers a block body, an expression body with a type alias, and an async block body.
Each program gives one S100, as the pin controls do.
The block and async bodies keep this message:

```text
type mismatch: the return value expects `i32`, got `string`
```

The expression body keeps this message:

```text
type mismatch: the lambda body expects `i32`, got `string`
```

The checker emits the existing `Callee::LambdaSelf` form.
Lowering and the execution tiers require no change.
The development tier, ship tier, and LIR interpreter equal the a450 golden.
Node equals that golden too.

The document generator rewrites the three generated documents.
Only `generated-docs/corpus-index.md` changes, with one a450 row.
The LIR snapshot tool adds 369 lines for a450.
A block comparison finds no change to any existing snapshot entry.
Moved entries: a450 is new; no existing entry moves.
The only new output golden is a450; no existing output golden changes.

### Measurements

All Cargo commands use offline dependencies and the pinned debug toolchain.
Command wall times include build time and concurrent artifact-lock waits.

| Command or case | Result | Wall time |
|---|---|---:|
| `cargo build --offline -p subscript-cli` at the pin | PASS; unchanged compiler sources | 0.180 s |
| Pin CLI, `check a450` | RED; exit 1; four S100 diagnostics | 0.613 s |
| `node_modules/.bin/tsc -p tsconfig.json` | PASS; includes a450 | 0.677 s |
| Node corpus runner, a450 | PASS; exit 0; four output lines | 0.136 s |
| Pin CLI, invalid block-result control | S100; expected exit 1 | 0.005 s |
| `cargo test --offline -p subscript-compiler --lib local_lambda_self_call` | PASS; 2 tests | 4.164 s |
| `cargo build --offline -p subscript-cli --bin subscript -p subscript-codegen --bin capture` | PASS | 3.952 s |
| Capture tool, a450 | PASS; writes the new four-line golden | 0.752 s |
| Debug CLI, a450 dev run | PASS; equals golden | 0.784 s |
| Debug CLI, a450 ship build | PASS | 0.225 s |
| `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1 cargo test --offline -p subscript-codegen --test lir coroutine_and_measurement_lir_text_matches_goldens` | PASS; 1 test | 5.904 s |
| `cargo run --offline -p subscript-compiler --bin generate-api-reference` | PASS; writes all three generated documents | 3.391 s |
| LIR snapshot block comparison and copy | PASS; only a450 adds an entry; 369 lines | 0.169 s |
| a450 ship executable | PASS; equals golden | 0.244 s |
| `cargo test --offline -p subscript-compiler --lib` | PASS; 614 tests | 4.359 s |
| `cargo test --offline -p subscript-compiler --test corpus_accept` | PASS; 13 tests | 10.378 s |
| `cargo test --offline -p subscript-compiler --test corpus_reject` | PASS; 49 tests | 3.231 s |
| `cargo test --offline -p subscript-compiler --test js_corpus` | PASS; 11 tests | 3.271 s |
| `cargo fmt --check` | PASS | 5.921 s |
| `cargo test --offline -p subscript-compiler --lib check::rejection_total` | PASS; 19 tests | 4.834 s |
| `cargo clippy --offline -p subscript-compiler --all-targets` | PASS; warnings outside the changed files | 11.298 s |
| `cargo test --offline -p subscript-codegen --test lir_completion --test undefined_result_lir --test local_lambda_self_call` | PASS; 11, 1, and 1 tests | 4.539 s |
| `cargo test --offline -p subscript-codegen --test golden` | PASS; 3 tests | 55.483 s |
| Pin CLI, invalid expression-result control | S100; expected exit 1 | 0.046 s |
| Pin CLI, invalid async-result control | S100; expected exit 1 | 0.009 s |
| `cargo test --offline -p subscript-codegen --test cemit` | PASS; 106 tests | 27.190 s |
| Scope, source size, old snapshot blocks, and `git diff --check` | PASS | 0.422 s |
| `cargo test --offline -p subscript-codegen --test lir` | PASS; 69 tests | 21.251 s |
| `tools/hygiene.sh` | PASS | 3.950 s |

The clippy warnings refer to unchanged files. Neither changed Rust file has a clippy warning.
No lint suppression was added.

| Final test result line | Test runner wall time |
|---|---:|
| unit: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 612 filtered out | 0.02s |
| lir-capture: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 68 filtered out | 4.46s |
| compiler-lib: ok. 614 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 2.50s |
| accept: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 9.60s |
| reject: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 2.12s |
| js-corpus: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 2.00s |
| rejection-total: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 595 filtered out | 4.68s |
| golden: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 52.62s |
| cemit: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 25.96s |
| lir: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 19.70s |
| lir-related: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 0.38s |
| lir-related: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 0.54s |
| lir-related: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 0.05s |

### Changed files and result

- `compiler/src/check/expr/lambda.rs`
- `compiler/src/tests/local_lambda_self_call.rs`
- `corpus/accept/a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.ts`
- `corpus/accept/a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation.expected`
- `codegen/tests/lir-goldens/corpus.txt`
- `generated-docs/corpus-index.md`
- `specs/tracking/local-lambda-self-call.md`

The changed Rust files contain 598 and 88 lines.
The requested tests, formatting check, and clippy command pass.
No contract block changes. No commit was made. `tools/gate.sh` was not run.
All background test commands from this round completed; none remains without a reader.

## Section 285, round 2: declared type and parameter count

Contract: compiler.md §285 rule 4 and acceptance item 4, including the self-call forms of §284.
Pin: `fed5720af17eba42c9a74ae140eab2df5dfb7440`.
The initial work tree contains the changes from round 1.
This round keeps those changes.

The local const initializer registers a self-call candidate only when its annotation permits the call.
An absent annotation permits the candidate.
A function annotation permits the candidate only when its parameter count equals the lambda parameter count.
A nullable annotation or another declared type does not permit the candidate.
This test precedes every self-call admission, including an explicit lambda result and a syntax-decided void block.
The existing pending-name lookup gives the required S100 when no candidate exists.
Lowering and the execution tiers require no change.

Four new unit tests cover the two expression-body defects and the two void-block variants.
Each test covers an inline annotation, a type alias, and the presence or absence of a lambda result annotation.
Each of the 16 recursive programs gives one S100 with this message:

```text
`rec` is read before its declaration in this block
```

The checker accepts all 16 controls without the self-call.
Before the source fix, each new test fails because the checker accepts its recursive program.
The focused tests pass after the source fix.

### Measurements

All Cargo commands use offline dependencies and the pinned debug toolchain.
The full checks run concurrently; command wall times include shared Cargo lock waits.

| Command or case | Result | Wall time |
|---|---|---:|
| `cargo test --offline -p subscript-compiler --lib local_lambda_self_call`, before the fix | RED; 2 passed; 4 failed; exit 101 | 3.46 s |
| `cargo test --offline -p subscript-compiler --lib local_lambda_self_call`, after the fix | PASS; 6 tests | 2.83 s |
| `cargo test --offline -p subscript-compiler --lib` | PASS; 618 tests | 9.554 s |
| `cargo test --offline -p subscript-compiler --test corpus_accept` | PASS; 13 tests | 7.921 s |
| `cargo test --offline -p subscript-compiler --test corpus_reject` | PASS; 49 tests | 8.509 s |
| `cargo test --offline -p subscript-compiler --test js_corpus` | PASS; 11 tests | 4.615 s |
| `cargo test --offline -p subscript-codegen --test lir` | PASS; 69 tests | 30.756 s |
| `cargo fmt --check` | PASS | 2.456 s |
| `cargo clippy --offline -p subscript-compiler --all-targets` | PASS; warnings outside the changed files | 20.595 s |
| `tools/hygiene.sh` | PASS | 5.703 s |
| `git diff --check` | PASS | 0.077 s |

The clippy warnings refer to unchanged files. No lint suppression was added.

| Test result line | Test runner wall time |
|---|---:|
| unit, before the fix: FAILED. 2 passed; 4 failed; 0 ignored; 0 measured; 612 filtered out | 0.02 s |
| unit, after the fix: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 612 filtered out | 0.02 s |
| compiler-lib: ok. 618 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 3.07 s |
| accept: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 4.23 s |
| reject: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 1.73 s |
| js-corpus: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 0.78 s |
| lir: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out | 18.99 s |

### Changed files and result

This round changes these files:

- `compiler/src/check/stmt.rs`
- `compiler/src/tests/local_lambda_self_call.rs`
- `specs/tracking/local-lambda-self-call.md`

The changed Rust files contain 1,973 and 159 lines.
The inherited `lambda.rs` change remains at 598 lines.
The requested tests, formatting check, and clippy command pass.
No contract block or output golden changes in this round.
No commit was made. `tools/gate.sh` was not run.
All test commands from this round completed; none remains without a reader.


## Section 285, round 3: apparent function shape

The rule 4 check now reads the annotation through `Checker::apparent_type`.
It admits a bare function only when its parameter count equals the lambda parameter count.
A nullable or union shape still fails that test.
The shape scan exempts identity and assignability checks under §143 rule 1b.
This site selects a value operation, so it requires the apparent type. No allowlist entry was added.

A new unit test uses `type G = ((k: i32) => i32) | null;`.
With and without a lambda result annotation, its self-call gives one S100:
`rec` is read before its declaration in this block.
Both controls without a self-call pass.
The test module now makes 45 checker calls.

All commands use the pinned toolchain. Cargo tests and clippy use offline dependencies.
Commands ran in sequence. Wall times cover each full command.

| Command | Result | Wall time |
|---|---|---:|
| `cargo test --offline -p subscript-compiler --test apparent_type_shapes` | PASS; ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s | 3.898 s |
| `cargo test --offline -p subscript-compiler --lib` | PASS; ok. 619 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.19s | 5.844 s |
| `cargo test --offline -p subscript-compiler --test corpus_accept` | PASS; ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.08s | 3.878 s |
| `cargo test --offline -p subscript-compiler --test corpus_reject` | PASS; ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s | 1.895 s |
| `cargo test --offline -p subscript-compiler --test js_corpus` | PASS; ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s | 1.206 s |
| `cargo fmt --check` | PASS; exit 0 | 2.052 s |
| `cargo clippy --offline -p subscript-compiler --all-targets` | PASS; exit 0 | 3.663 s |

This round changes these files:

- `compiler/src/check/stmt.rs`
- `compiler/src/tests/local_lambda_self_call.rs`
- `specs/tracking/local-lambda-self-call.md`

The Rust files contain 1,973 and 180 lines.
Inherited changes remain in the work tree.
No contract block, golden, or lowering change was made in this round.
No commit was made. `tools/gate.sh` was not run.
All test commands from this round completed.

Clippy exits 0 with warnings outside the two changed Rust files.
`git diff --check` passes; exit 0; wall time 0.046 s.

## §285 full gate

The first full gate on the §285 tree failed one test in each profile:
`every_value_shape_test_uses_the_apparent_type` (§143 rule 1a). The
rule 4 check read the shape of the declared type, not of the apparent
type. The check now reads the apparent type, and a unit test pins that
an alias of the nullable form stays rejected. The second run passed:

```text
gate full fed5720af17eba42c9a74ae140eab2df5dfb7440 dirty:9 debug 3402/0/5 release 3383/0/5 skips 1/0 clippy 3/10/13 goldens-moved 1 exit 0
```

The moved golden is the LIR text snapshot. It grows by the functions of
`a450` only.
