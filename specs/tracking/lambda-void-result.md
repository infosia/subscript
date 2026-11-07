# A lambda with no value result is `void` — `compiler.md` §238

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code base `7b2e8668` (the contract pin
`538f7155` changed specs only): `subscript check
a425-lambda-with-no-value-result-is-void.ts` gave 14 errors, each
S100, then `error: 14 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 43:26 | `w1().then(() => v())` in a `Promise<void>` return | a `void` expression is only allowed as an expression statement |
| 47:19 | `const f = () => v();` | the same |
| 49:26 | `const af = async () => v();` | the same |
| 51:13 | `const g = () => { v(); };` | a lambda with a block body requires a return type annotation |
| 55:17 | a block with a bare `return` in an `if` | the same |
| 64:17 | a block with a nested lambda that returns a value | the same |
| 71:19 | `w1().then(() => { v(); })` | the same |
| 74:31 | `wi().then((x: i32) => v())` | a `void` expression is only allowed as an expression statement |
| 77:11 | a `void` callback after a value callback in a `then` chain | a lambda with a block body requires a return type annotation |
| 82:24 | `each(xs, (x: i32) => v())` | a `void` expression is only allowed as an expression statement |
| 83:12 | `each(xs, (x: i32) => ys.push(x))` | type mismatch: the argument expects `(i32) => void`, got `(i32) => i32` |
| 84:12 | `each(xs, (x: i32) => { … })` | a lambda with a block body requires a return type annotation |
| 88:24 | a generic method `visit<T>` with a block callback | the same |
| 91:33 | the generic method with `(x: i32) => v()` | a `void` expression is only allowed as an expression statement |

## After the change

`a425` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes.

## Form

- `compiler/src/check/expr/lambda.rs`. A lambda with no result type
  and a block body with no `return` operand has the result `void`, or
  `Promise<void>` when it is async. A `return` in a nested function,
  lambda, or class does not count. A block with a `return` operand
  keeps `BlockLambdaReturnAnnotationMissing`.
- An expression body with no result type is checked with its `void`
  type kept (`check_expr_keeping_void`). A `void` body is a statement,
  and the lambda has the result `void`.
- `compiler/src/check/inference.rs`. In a generic call, an arrow
  argument whose parameter type is a function type with the result
  `void` takes `void` as the context of its result, when the parameter
  type has no context. §237 rule 4 then discards a value body.
- The witness program `new-shared-nullable-call` has the block lambda
  `()=>{}`, which is now accepted. Its row for
  `BlockLambdaReturnAnnotationMissing` is removed; the program still
  witnesses `NullableCallShared`. The site keeps the witness `a-s105`
  (`() => { return 1; }`).
- Two tests in `compiler/tests/initializer_inference.rs` used a block
  lambda with no `return` operand as a rejected form. Each now adds
  `return 1;`, so the form stays rejected and the test reads the same
  fact.

## Gate

One run after the implementation, and one after the Phase Review fix:

- `gate quick bf0048f1 dirty:15 debug 3246/0/5 skips 2 goldens-moved 1 exit 0`
- `gate quick 538f7155 dirty:15 debug 3246/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a425`
block only.

## Phase Review

No CRITICAL or MAJOR finding. One MINOR finding was fixed, and the
others were recorded, in §238.3. Open items are in §238.4. Hygiene:
clean.
