# A nullable function type gives a lambda its parameter types — `compiler.md` §251

Status: COMPLETE.

## Red at the pin

The debug CLI of the code of `ef48b6e9` (the commits after `834116f9`
changed specs only), run on a `$TMPDIR` copy:

`subscript check a436-a-nullable-function-type-gives-a-lambda-its-parameter-types.ts`
gave 11 errors, then `error: 11 error(s)`. Each is S100 "parameters
require a type annotation", at 22:11 (`retn`), 46:25 (`const f`), 49:8
(`g = …`), 52:9 (`take`), 53:35 and 55:35 (`three`, in each order),
58:29 (the branch), 61:16 (the member), 63:31 (the element), 67:15
(`m.set`), and 71:40 (`const d`, with a result context).

## After the change

`a436` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`f 1 g 2 r 3 t 4 h 5 k 6 c 7 hc 8 xs 9 m 11 d 42`, one per line.
`xs[1]` is `null`, so `call(xs[1], 10)` prints nothing. `main` is not
async, so the LIR text snapshot does not change.

Rules 3 and 5, measured with the CLI of the pin and the CLI after the
change:

| Form | Pin | After |
|---|---|---|
| `const a: CB \| null = (x: i32) => …`, `const b: CB \| null = nv`, `null` | runs | the same |
| `const d: ((x: i32) => f64) \| null = (x: i32) => 7 / 2`, parameters annotated, result not | S100 "type mismatch", got `(i32) => i32` | runs, `d(1)` is `3.5`: the lambda takes the result context `f64`, as in a place `(x: i32) => f64` |
| `[maybe(), (x) => …]`, a nullable hint | S100 | the same |
| `const k = (x) => …`, no context | S100 | the same |
| `const q: CB \| null = (x: string) => …` | S100 type mismatch | the same |
| `const w: ((x: i32) => Promise<void>) \| null = async (x) => { … }` | S100 | runs |
| `const r: CB \| null = (x) => x + 1` | S100 | runs; the result is discarded, as in a place `CB` (§242) |
| `const z: ((x: i32) => i32) \| null = (x) => print(…)` | S100, parameters | S100, a `void` expression, as in a place `(x: i32) => i32` |

## Form

- `compiler/src/check/expr/lambda.rs`. `lambda_context` is the one
  place that derives the function type of a lambda from its contextual
  type: a function type, or the function type `F` of `F | null`.
  `check_lambda` reads it. A hint (§249 rule 1) gives no parameter
  types, also when it is nullable. The place check then takes the
  lambda of the type `F` into the place `F | null`, as it takes an
  annotated lambda.
- The unit test of §249 `element_hint_lambda_parameters.rs` pinned
  `three(one, maybe(), …)` and the other order as S100 (§249.4 item 3).
  It now accepts both.

## Files

- `compiler/src/check/expr/lambda.rs`
- `compiler/tests/nullable_function_lambda_context.rs` (new),
  `compiler/tests/element_hint_lambda_parameters.rs`
- `corpus/accept/a436-a-nullable-function-type-gives-a-lambda-its-parameter-types.ts`
  and `.expected`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s249-an-element-hint-gives-no-parameter-types-to-a-lambda.md`
  (§249.4 item 3 closed)
