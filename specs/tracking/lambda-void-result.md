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

## §286: a `return` of a `void` operand

Contract: `specs/blocks/compiler/s286-a-return-of-a-void-operand-keeps-a-lambda-void.md`.

### Red

At the contract commit `f131811d`, `subscript check` of
`a451-a-return-of-a-void-operand-keeps-a-lambda-void` gave five S100
"a lambda with a block body requires a return type annotation", one at
each lambda of lines 11 to 15 (`h`, `a`, `z`, `rec`, `nested`).

### Form

- `compiler/src/check/expr/lambda.rs`. A block lambda with no result
  annotation and no context result takes the result `void` (or
  `Promise<void>`) before its body is checked. Its frame carries
  `inferred_void`: the lambda position and whether the diagnostic was
  given.
- `compiler/src/check/stmt.rs`. A `return` with an operand in such a
  frame calls `check_inferred_void_return`. The operand is checked as a
  `void` operand with no context. If its apparent type is not `void` or
  `Error`, the lambda gets `BlockLambdaReturnAnnotationMissing` once at
  its position, and the lambda result becomes `Error`.
- Rule 3a: before the result is taken, `returns_nonvoid_syntax` reads
  each `return` operand. An operand that never has type `void` gives
  the diagnostic at once, and the lambda has the type `Error`.
- Rule 3b: a conditional operand takes `split_return`, the §237 rule 2
  path of a function with the result `void`.
- The syntax scan `returns_a_value` is removed. `DeferredWork::Lambda`
  carries `inferred_void`, so a deferred body (§239) applies the same
  rule.
- `a451` output, checked by hand: `v h w1 a v r2 r1 v inner=7 v`.
- `generated-docs/corpus-index.md` gains the `a451` row.

### Phase Review

One fresh review. One MAJOR and three MINOR findings fixed, one
recorded; see §286.3. Hygiene: exit 0.

### Gate

- `gate full 02049170 dirty:11 debug 3404/1/5 release 3385/1/5 skips 1/0 clippy 3/10/13 goldens-moved 1 exit 1`.
  The failure: `initializer_inference.rs`
  `a_rejected_block_result_still_checks_non_result_statements`. The
  rule 3a branch returned before the body was checked, so the S009 of
  a capture was lost. Rule 3a now takes the result `Error` and checks
  the body. The second input of the test, `return missing;`, now gives
  S009 and S016 "unknown name `missing`", by the last sentence of rule
  3a; the test asserts each.
- `gate full 8502bf32 dirty:12 debug 3405/0/5 release 3386/0/5 skips 1/0 clippy 3/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a451`
  section only.

## §287: an async `void` lambda returns a `Promise<void>`

Contract: `specs/blocks/compiler/s287-an-async-void-lambda-returns-a-promise-of-void.md`.

### Red

At the contract commit `d41d74fc`, `subscript check` of
`a452-an-async-void-lambda-returns-a-promise-of-void` gave three S100:
"a lambda with a block body requires a return type annotation" at
lines 10 and 11, and "the initializer expects `Promise<void>`, got
`Promise<<error>>`" at line 13.

### Form

- `compiler/src/check/expr/lambda.rs` `check_inferred_void_return`. In
  an async frame, an operand of type `Promise<void>` takes
  `adopt_returned_handle`, the path of a `return` in an async lambda
  annotated `Promise<void>`.
- `a452` output, checked by hand: `w1 g w1 w1`.
- `generated-docs/corpus-index.md` gains the `a452` row.

### Phase Review

One fresh review. Three MINOR findings, each fixed; see §287.3.

### Gate

- `gate full d41d74fc dirty:8 debug 3407/0/5 release 3388/0/5 skips 1/0 clippy 3/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a452`
  section only.
