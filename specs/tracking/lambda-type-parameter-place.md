# A lambda in a type parameter place names no TypeScript form — `compiler.md` §252

Status: IN PROGRESS.

## Red at the pin

The debug CLI of the code of `3a064791` (the contract commit changed
specs only after `cf1b2d5f`), run on a `$TMPDIR` copy:

`subscript check r498-a-lambda-in-a-type-parameter-place-names-no-typescript-form.ts`
gave 3 errors, each S100 "parameters require a type annotation" with the
block "TypeScript accepts:" of `GenericCallbackParameterAnnotationMissing`:
26:9 (`wrap`), 27:22 (`new Box`), and 28:7 (`nn`). `tsc` 5.9.2 gives
TS7006 at the same three positions.

With the row of `r498` in `compiler/tests/corpus_reject.rs`, the test
`divergence_blocks_match_every_reject_entry_tsc_header` failed at the
code of the pin: "r498-a-lambda-in-a-type-parameter-place-names-no-typescript-form.ts:
tsc rejects but the divergence block is present".

## After the change

`r498` gives the same three S100, at the same positions, with no block.
`divergence_blocks_match_every_reject_entry_tsc_header` passes.

Measured with the CLI after the change:

| Form | Block | `tsc` |
|---|---|---|
| `wrap((x) => …)`, `new Box((x) => …)`, `nn((x) => …)` | none | TS7006 |
| `[mk, () => wrap((x) => …)]` (`r497`) | none | TS7006 |
| `wrap<i32>((x) => …)`, `new Box<i32>((x) => …)` | none | TS2345 and TS7006 |
| `lone((x) => …)`, `each([1, 2], (x) => {})`, `fn((x) => …)` | `GenericCallbackParameterAnnotationNeeded` | accepts |
| `lone<i32[]>(([a]) => {})`, `wrap<(x: i32[]) => void>(([a]) => {})` | `GenericCallbackParameterAnnotationNeeded` | accepts |
| `wrap<CB>((x, y) => {})`, `new Box<CB>((x, y) => {})`, `lone((x, y) => {})`, `lone<i32>((x, y) => {})` | none | TS2345 and TS7006 |
| `lone<i32>((x) => {})`, `wrap<CB>((x) => …)` | accepted | accepts |

A binding pattern parameter takes no type from the context, so the two
pattern forms reach the diagnostic with explicit type arguments. They
are the forms of the branch in `call.rs` that keeps the block.

## Form

- `compiler/src/check/inference.rs`. `generic_callback_place` is the
  one predicate of rule 1: a lambda in a place of a function type `F`, or
  `F | null`, with no more parameters than `F`. Each writer of
  `generic_callback_context` gives it the count of the parameters of `F`.
- `declared_function_arity` (`inference.rs`) reads that count from the
  declared annotation, before the substitution: a function type, `F |
  null`, or a type alias that `lambda_context` (§251) reads as a function
  type. A type parameter names no function type. `infer_arguments`, the
  inference of a generic function, a generic method, and a generic
  constructor, uses it.
- `check_args_with_arguments` (`compiler/src/check/expr/call.rs`) reads
  the count from the type of the place after the substitution, by
  `lambda_context`. That path checks a call with explicit type
  arguments.
- The unit tests of `initializer_inference.rs` pinned the block for a
  lambda in a place `T`: `g((x) => 1)`, `g<i32>((x) => 1)`, `g(([x]) => 1)`
  with `g<T>(v: T)`, and `pair((x) => x + 1, (x) => x + 1)` with
  `pair<T>(a: T, b: T)`. They now pin no block, and the first test has
  the name `a_lambda_in_a_type_parameter_place_has_no_block`.

## Files

- `compiler/src/check/inference.rs`, `compiler/src/check/expr/call.rs`
- `compiler/tests/lambda_type_parameter_place.rs` (new),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/initializer_inference.rs`
- `corpus/reject/r498-a-lambda-in-a-type-parameter-place-names-no-typescript-form.ts`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s249-an-element-hint-gives-no-parameter-types-to-a-lambda.md`
  (§249.4 item 5 closed)
