# §164: exit predicate and required arguments

The checker and the lowering use one statement-exit predicate.
Literal conditions take one CFG edge.
The checker counts required arguments through the last parameter without a default.
Inferred local and global bindings carry the source required count.
The function-value argument site splits on that count.

## Contract-pin measurements

The contract pin is `e9353dcd`.
`cargo build --offline --locked -p subscript-cli` passes at that pin.
Each CLI input is a temporary copy of its corpus entry.
TypeScript 5.9.2 uses a temporary config that extends the repository config and includes the prelude and one entry.

| Entry | TypeScript | Pin checker | Pin run |
| --- | --- | --- | --- |
| `a334-exit-predicate` | Accepts | Rejects four functions with S100 | Exit 1, the same four errors |
| `r375-default-before-required` | Rejects with TS2554 at 10:12 | Accepts, exit 0 | Exit 2: missing argument `b` with no default |
| `r376-function-value-default` | Accepts | Rejects with S100 at 10:12, without a divergence block | Exit 1, the same diagnostic |

The pin gives `r376` this message:

```text
`the function value` expects 2 argument(s) (2 required), got 1
```

TypeScript accepts each isolated `a334` function below.
Each isolated program calls its function with `2` and catches its exception.

| Function | Pin run |
| --- | --- |
| `row_while` | Exit 2: non-void function has a reachable fallthrough |
| `row_for_true` | Exit 2: the same internal error |
| `row_conditional` | Exit 2: the same internal error |
| `row_throw` | Exit 2: the same internal error |
| `row_switch_loop` | Exit 2: the same internal error |
| `row_try` | Exit 2: the same internal error |
| `row_using` | Exit 2: the same internal error |
| `row_for_absent` | Exit 0, output `2` |
| `row_if_true` | Exit 1: S100, not all paths return a value |
| `row_if_false` | Exit 1: the same diagnostic |
| `row_grouped` | Exit 1: the same diagnostic |
| `row_else_true` | Exit 1: the same diagnostic |

Node runs the emitted CommonJS entry with `print` bound to `console.log`.
Its output defines the new `a334` golden:

```text
3
4
5
throw
2
7
dispose
8
9
10
11
1
2
```

## Function-exit comparison

The exact comparison failed on `a257`, function `returnInsideTry`; rule 4 changed to one direction.
The lowering rejects a reachable CFG end when the body predicate answers no.
The reverse difference is legal because the predicate includes a catch handler without a raise edge.
The CFG walk includes instruction handler edges and terminator edges.
The comparison applies to void, non-void, generator, async, lambda, constructor, and method bodies.
A unit test leaves the builder entry open while its unchanged body cannot leave its end.
The comparison rejects that form and accepts the same builder with the lowered body.
A void `try { return; } catch (e) {}` control passes.

## Other form facts

The checker rejects a closed alias switch with a missing member before it emits HIR.
For `"a" | "b"`, cases for both members pass the pin checker and TypeScript.
With the same cases and an extra member `"c"`, the checker rejects the missing case, and TypeScript reports TS2366.
This checked-HIR constraint supplies the closed-alias coverage fact of rule 2.

## Corpus results

The original pin is `e9353dcd`; the round 2 contract pin is `361485f4`.
The compiler and codegen sources are identical at these pins.
The Red measurements above remain unchanged.
A CLI build from `361485f4` passes with `--offline --locked`.
Temporary copies of all three entries reproduce the retained pin results.
No existing reject entry changes its expected text or result.
No existing `.expected` golden changes.
The new `a334` golden remains the Node output above.

These corpus functions change their LIR text:

- `a146-scoped-locals / synchronousScopes`
- `a149-suspension-state / unreachableAfterBreak`
- `a149-suspension-state / main`
- `a20-coroutine-generator / main`
- `a215-user-view-method-names / main`
- `a216-generator-escapes-its-call / drain`
- `a217-generator-in-a-class-field / main`
- `a223-generator-in-an-array / main`
- `a225-generator-exhausted-in-storage / main`
- `a259-try-holds-yield / main`
- `a310-generator-done-reference-controls / main`
- `a327-void-generator / steps`
- `a327-void-generator / main`
- `a329-initializer-inference / main`
- `a79-for-of-generator / main`

The corpus-index generator adds `a334`, `r375`, and `r376`.
No other generated file changes.

## Tests

The exit tests cover each contract shape and a same-shape control.
Each exit shape also has a `using` scope control.
The branch tests count edges for literal and variable conditions.
The argument tests cover declaration counts and inferred local and global bindings.
The binding tests cover lambda defaults, function names, binding aliases, annotations, and calls below the source required count.
The §154 table adds the optional-argument site and its TypeScript-accepted witness.

## Validation

`cargo test --offline --locked -p subscript-compiler` passes.
`cargo test --offline --locked -p subscript-codegen` passes.
JIT, C AOT, interpreter, and corpus golden comparisons pass.
The §154 total test passes.
TypeScript 5.9.2 accepts `a334` and `r376`; it rejects `r375` with TS2554 at 10:12.
Node emits the unchanged `a334` golden.
The final CLI accepts and runs a temporary `a334` copy with the same golden.
The final CLI rejects temporary `r375` and `r376` copies with the contracted S100 messages.
The `r376` diagnostic carries the C24 divergence block and the function-type note.
`cargo fmt --check` passes.
`cargo clippy --offline --locked --workspace --all-targets` passes with no new warning.
The library warning counts are 18 for runtime, 5 for compiler, and 13 for codegen.
`tools/hygiene.sh` passes.
Every changed Rust file has at most 2,000 lines.

## Round 3 findings

- Unreachable loop bodies do not lower. Four false-loop cases and four variable-condition controls give `1,1,1,1,3,3,3,3` on TypeScript JavaScript, JIT, C AOT, and interpreter.
- Assignments preserve each inferred binding's required count. All 36 local/global cases match TypeScript 5.9.2: required values reject with S100/TS2322; default-value controls pass.
- Generic inference counts through the last required parameter. `f<T>(a:i32=1,b:T)` with `f(2)` gives S100, `TscRejects`, and TS2554; both same-shape controls pass.
- The divergence `why` states only the C7 reason. The rendered diagnostic test keeps the note and finds the argument advice once.
- The loop comment includes literal `true`. The predicate module comment names all three rule 1 consumers; exit-shape and branch-edge tests pass.

Before the fixes, the added tests reproduced all three defects in the round 2 implementation.
Four false-loop bodies failed LIR dominance checks.
The assignment checker accepted the required lambda.
The generic call carried `GenericInferenceMissing` instead of `TscRejects`.
TypeScript 5.9.2 checks all 36 assignment cases and three generic cases with the repository prelude.
Node runs all eight loop cases from TypeScript-emitted CommonJS.

The round 3 workspace build passes with `--offline --locked --workspace --all-targets`.
The full compiler and codegen test suites pass with `--offline --locked`.
The §154 total tests, corpus comparisons, and LIR golden tests pass.
`cargo fmt --check` passes.
Workspace clippy passes with no new warning; library counts remain runtime 18, compiler 5, and codegen 13.
All 36 changed Rust files stay within 2,000 lines.

## Phase Review

The first review found two MAJOR findings and five MINOR findings; round 3 fixed each one.
M1: `while (false)` and `for (; false;)` bodies that reach their end failed the LIR dominance check.
M2: an assignment to a binding with the rule 7 fact accepted a value with a required `b` (TS2322); present before §164.
The exact rule 4 comparison failed on `a257` `returnInsideTry`, so rule 4 changed to one direction.
The second review found no CRITICAL or MAJOR finding; its MINOR findings are §164.4 items 2 to 5.
The full gate at `a15d3f94` reports debug 2371/0/3, release 2368/0/3, goldens-moved 1 (the LIR text golden), exit 0.

## Changed files

- `codegen/src/lir/builder.rs`
- `codegen/src/lir/stmt.rs`
- `codegen/tests/exit_predicate.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/support/lir_facts.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/fallthrough.rs`
- `compiler/src/check/function_value.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/initializer.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_diagnostic.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/diag.rs`
- `compiler/src/diag_render.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/exit_predicate_and_arguments.rs`
- `corpus/accept/a334-exit-predicate.expected`
- `corpus/accept/a334-exit-predicate.ts`
- `corpus/reject/r375-default-before-required.ts`
- `corpus/reject/r376-function-value-default.ts`
- `generated-docs/corpus-index.md`
- `specs/tracking/s164-exit-predicate-and-arguments.md`
