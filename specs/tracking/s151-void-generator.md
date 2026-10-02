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
