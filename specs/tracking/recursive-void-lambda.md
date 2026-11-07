# A recursive `void` lambda decides its type from its syntax — `compiler.md` §239

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code base `00b8b7b2` (the contract pin
`9150ec6d` changed specs only): `subscript check
a426-recursive-void-lambda.ts` gave 4 errors, each S100 "initializer
type depends on its own undecided type", then `error: 4 error(s)`:

| Line:col | Form |
|---|---|
| 15:14 | `rec(n - 1)` in `const rec = (n: i32) => { … }` |
| 24:3 | `ping(n)` in `pong`, the mutual pair |
| 30:11 | `await arec(n - 1)` in the async lambda |
| 37:14 | `count(n - 1)` in the lambda that writes the module-level `let total` and calls `note` |

## After the change

`a426` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes.
The module-level `let` write and the call of a module function needed
no other form.

## Form

- `compiler/src/check/expr/lambda.rs`. One fact,
  `decided_from_syntax`, is true for a result annotation and for a
  block body with no `return` operand (§238 rule 1). While the checker
  decides the type of an initializer, each such lambda takes the
  deferred path: its type comes from its parameters and that result,
  and `DeferredWork::Lambda` checks its body later with the same
  result. The body gets the same frame facts as on the direct path,
  because `check_lambda_body` derives them from the same result and
  the same syntax.
- An expression body decides its result from its type, as before.

## Gate

One run after the implementation:

`gate quick 0cd4193c dirty:8 debug 3248/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a426`
block only.

## Phase Review

No CRITICAL or MAJOR finding. One MINOR finding is recorded in §239.3.
Open items are in §239.4. Hygiene: clean.
