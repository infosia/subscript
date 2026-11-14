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
