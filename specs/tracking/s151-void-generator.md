# §151 — void generator

Contract: `specs/blocks/compiler/s151-a-void-generator-runs-on-every-tier.md`.
Contract pin: `b8d10e21`. Implementation round: 1. No commit.

## Source search

Before the change, a multiline search covered `corpus/`, `examples/`, `docs/`, `generated-docs/`, and compiler/codegen tests.

The only bare `yield;` was `corpus/reject/r70-generator-frame-layout-too-large.ts:11`.
Its declared element type is `void`. No non-void site required a stop.

## Red evidence

The measurement used compiler and codegen binaries built from an archive of `b8d10e21`.
The pin source had no production edits. A temporary measurement test supplied the new corpus sources.

`a327-void-generator` covers explicit `next()`, `for...of`, early return, class-method execution, and `while (!r.done)`.
The class method calls and drives a void generator. Generator methods and nested function declarations remain outside the decided surface.

| Witness at the pin | Measured result |
|---|---|
| dev | Matches the complete golden below. |
| ship | `Internal("internal lowering error: internal error: type tag for Void")` |
| interpreter | Output `tick 0\n`, then `InvalidLir`: `a generator suspension is not a yield with a value`. |
| reject checker | Accepts `r334-bare-yield-nonvoid`. |

The pin CLI C emission also reports `subscript: internal error: type tag for Void`, exit 1.

Node v24.18.0 supplied the new golden. TypeScript 5.9.2 accepts `a327` with the repository options.
TypeScript rejects `r334` with TS2322: `Type 'undefined' is not assignable to type 'number'.`

```text
tick 0
false
tick 1
false
true
tick 0
step
stopped true
early
early false
early true
early true
method 0
method step
method 1
method step
tick 0
while step
tick 1
while step
while true
```

## Change

`SuspendKind::Yield(None)` is the single LIR form for a value-less generator suspension.
The LIR documentation states its void element requirement. The verifier rejects this form in a non-void generator.
The dev lowering and the C emitter consume this form. Their void `next()` calls pass a null output pointer.
The interpreter accepts this suspension and returns a step result with `done = false`.
The void result has no value storage. The C result contains only `done`; a void loop binding emits no value read.
The checker rejects a bare yield when the known element type is non-void. Its diagnostic names that type.

The new tests inspect the suspension form and the C result storage. A verifier test replaces a valued suspension with a value-less suspension.
The existing corpus tests provide execution parity. No added test repeats the corpus entry across three forms.

The LIR snapshot adds only the `a327-void-generator` section: 459 lines, 34,458 bytes.
Removing that section reproduces the previous snapshot byte for byte. No existing `.expected` file changed.
The API reference command regenerated `generated-docs/`; only the two new corpus index rows changed.
No runtime file or C runtime header required a change.

## Verification

- The joint `cargo test --offline --locked -p subscript-compiler -p subscript-codegen` run passes every codegen unit and integration target.
- That run stops at the compiler header parser. The reject header now uses the required `tsc: rejects TS2322` form.
- `cargo test --offline --locked -p subscript-compiler` passes all targets after that correction, including the TypeScript corpus and doc tests.
- `cargo test --offline --locked -p subscript-codegen --doc` passes. All codegen test targets therefore pass.
- The golden sweep compares 315 entries on dev and ship, with zero skips. The corrected implementation passes this sweep.
- The interpreter sweep matches all 251 selected entries. It declares 64 header exclusions and omits the debug benchmark entry `a22`.
- The new `a327` runs in the golden and interpreter sweeps. Each witness matches the Node golden.
- `cargo test --offline --locked -p subscript-compiler --test js_corpus` passes all 11 tests.
- `cargo test --offline --locked -p subscript-codegen --test void_generator` passes both tests in 0.02 seconds.
- The checker tests verify the declared element names `i32` and `Cell`. The apparent-type scan passes.
- `node node_modules/typescript/bin/tsc -p tsconfig.json` passes.
- `cargo fmt --check` and `git diff --check` pass.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference` completes after the final header correction.

No gate script ran. No commit was made.

## Landing gate

`goldens-moved 1` is the LIR text snapshot: `a327` is a generator entry.

```text
gate full 5f31dc196d43ed5066587f456de60eefd9d5fa7b dirty:16 debug 2229/0/3 release 2226/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Round 2 — void positions

The source search covered the corpus, examples, documentation code blocks, and compiler/codegen tests.
No accept entry or example used a rule 5 form.
The existing source sites were:

- `r297`: an inferred binding from a void call.
- `r298`: an array-map callback with a void result.
- `r299`: a void parameter and its return as a value.
- `compiler/tests/opaque_generics.rs`: four declarations with `T extends void`, including an array parameter and a value-class field.
- `compiler/src/tests/collections.rs`: `Map<void, i32>` annotations and constructor type arguments in the key-kind rejection test.
- `compiler/tests/generic_tsc_matrix/kinds.rs`: the void constraint seed generates void parameters, bindings, container elements, and call operands.
- The function-parameter seed generates void call results that the matrix uses as operands.

The new entries have measured `tsc: accepts` headers. The measurement used TypeScript 5.9.2 and the repository compiler options.
A compiler built from `b8d10e21` measured the new sources:

| Entry | Form | Result at the pin |
|---|---|---|
| `r335` | Annotated void binding | Accepted |
| `r336` | Void parameter | Accepted |
| `r337` | Void array element | Accepted |
| `r338` | Void call as a return operand | Existing S100/C21 rejection: `a void function cannot return a value` |
| `r339` | Void call as a yield operand | Accepted |
| `r340` | Void call as an argument | Accepted |
| `r341` | Void loop binding as an argument | Accepted |
| `r342` | Void generator value as an argument | Accepted |
| `r343` | Void in another Generator type-argument slot | Accepted |

Type resolution now rejects void outside a return, generator element, or Promise result slot.
Expression resolution now rejects a void operand. Both checks emit S100 with C21 and return an error type.
The inferred-binding check no longer needs its separate void branch.
A return check suppresses its second diagnostic when the operand already has an error.
A bare yield with an erroneous element type emits no second diagnostic.
A rejected generic constraint suppresses its body check. This prevents element-kind diagnostic cascades after the C21 rejection.

`r299` now reports its void parameter at line 8. Its header and checker row use that line.
The old void-constraint acceptance tests and Map key diagnostic test now use S100/C21.
The generic matrix uses the two common C21 diagnostic records.
A concrete control with a C21 rejection admits no instance.

The C emitter's empty IterValue return remains reachable from legal expression statements.
Two temporary programs measured `tick().next().value;` and `x;` inside a void-generator loop.
Each program contained one IterValue load and printed `ok` on dev, ship, and the interpreter.
The temporary test was removed. The emitter branch remains because those statements still reach it.

The two permanent void-generator shape tests took 0.02 seconds in a debug run.
Their module comment records that measurement.
No runtime file or C runtime header changed.

The matrix measurement keeps every previous omitted product instance and adds 16 void-parameter instances.
The product omission count moves from 7,719 to 7,735.
The destination omission count moves from 1,448 to 1,488.
The API omission count moves from 8,775 to 8,798.
The revised matrix checks 38,831 cells with zero failures.

Round 2 verification:

- `cargo test --offline --locked -p subscript-compiler -p subscript-codegen` passes all unit, integration, and documentation tests.
- The dev/ship golden corpus sweep and the interpreter corpus sweep pass.
- The reject corpus checks the new entries with S100/C21 at their expected lines.
- The JavaScript corpus tests and the TypeScript corpus header measurements pass.
- `cargo fmt --check` and `git diff --check` pass.
- `generate-api-reference` regenerates the corpus index with `r335`–`r343` and the revised `r299` description.
- No accept golden or LIR snapshot changes. No gate script ran. No commit was made.

C21 index additions for the orchestrator: `r335`, `r336`, `r337`, `r338`, `r339`, `r340`, `r341`, `r342`, `r343`.

## Phase Review fix round landing gate

A `void` value is never an operand (§151 rule 5, owner decision; C21
rewritten as a total rule). `r299` now first reports its `void`
parameter: under rule 5 a `void` function cannot reach a `void` value to
return, and `r338` pins `return f()`.

```text
gate full 1d628be1f03dd8742d12eb2ffbd538093ada29f1 dirty:29 debug 2230/0/3 release 2227/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review result

One review pass found MAJOR 1 (`yield f()` with a `void` call failed
lowering on every tier) and MINOR 5. MAJOR 1 and MINOR 2–4 were one
class, a `void` value used as an operand; rule 5 closes it. The cascade
diagnostic and the missing cost line are fixed. §151 is COMPLETE.
