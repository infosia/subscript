# A `void` function returns a `void` operand — `compiler.md` §237

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code base `ea723f61` (the contract pin
`14d25d0c` and its earlier versions changed specs only): `subscript check
a424-void-function-returns-a-void-operand.ts` gave 12 errors, each
S100, then `error: 12 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 47:10 | `return v();` | a `void` expression is only allowed as an expression statement |
| 50:31 | `(): void => v()` | the same |
| 53:10 | `return await settledWork();` | the same |
| 57:3 | `return settledWork();` | a `void` function cannot return a value |
| 61:3 | `return pendingWork();` | the same |
| 65:3 | `return failingWork();` | the same |
| 69:30 | `return b ? settledWork() : v();`, the `void` leaf | a `void` expression is only allowed as an expression statement |
| 75:12 | `return v();` in `try`/`finally` | the same |
| 99:26 | `xs.forEach((x: i32) => g(x))` | the same |
| 100:26 | `xs.forEach((x: i32) => ys.push(x))` | type mismatch: the lambda body expects `void`, got `i32` |
| 101:44 | `(x: i32) => ys.push(x * 10)` with a `(x: i32) => void` context | the same |
| 105:5 | `return ys.push(x * 100);` in a contextual block body | a `void` function cannot return a value |

## After the change

`a424` checks with no errors. It also has an all-`void` conditional,
`return b ? v() : g(9)`, called with each value of `b`. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes.
Measured order, with a ticker that prints one line per turn:

| Form | Result line |
|---|---|
| `return await settledWork()` | after `turn3` |
| `return settledWork()` (adoption) | after `turn4` |
| `return pendingWork()` (adoption, two turns) | after `turn5` |
| `return failingWork()` (adoption, rejects) | the caller catches after `turn4` |
| mixed leaf, handle branch | after `turn4` |
| mixed leaf, `void` branch | after `turn2` |

## Form

- `compiler/src/check/stmt.rs`, `check_void_return`. A conditional
  splits into one return per leaf, unless each leaf of an async
  function is a handle. A declared `void` result reads the type of the
  operand as an expression (`check_void_operand`): an assignment has the
  type of its value, and an optional chain has the type of its member or
  its call. A `void` operand is its statement and then a bare `return`.
  Another operand keeps the S100.
- A constructor and a setter keep the S100 of `return e`: the frame
  carries `rejects_return_operand`. A `[Symbol.dispose]` method accepts
  a `void` operand.
- `(e)` has the type of `e`. The leaves of a split conditional give one
  S100, at the `return`. A rejected optional async call reads its
  handle, so S013 does not fire.
- A lambda that is not async, with no annotation and a `void` result
  from its context, checks the operand as an expression statement and
  discards its value. An async lambda with a `Promise<void>` context
  keeps the S100 for a value.
- An async `Promise<void>` result adopts a `Promise<void>` handle with
  the form of §236: the placeholder return has no value, and the
  epilogue awaits the handle as a statement, then returns with no
  value.
- An arrow expression body with a `void` result is checked as a
  `return` of the body.
- The site `VoidFunctionReturnValue` keeps no form. The site, its
  witness, its target row, and its program are removed. The divergence
  `VoidValue` stays for its other sites; its text no longer names a
  bare return.
- `r338` is retired; its form is in `a424`.
- A test of §212 rule 1 that pinned the rejection of
  `run(() => id(1))` now accepts that form; `zero()` keeps the
  rejection with no candidate.

## Gate

One run after the implementation, and one after each Phase Review fix:

- `gate quick 9fc1f0ed dirty:20 debug 3236/0/5 skips 2 goldens-moved 1 exit 0`
- `gate quick 14b2840d dirty:22 debug 3239/0/5 skips 2 goldens-moved 1 exit 0`
- `gate quick d9f4d898 dirty:26 debug 3242/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a424`
block only.

## Phase Review

Four MAJOR and four MINOR findings were fixed, and three MINOR
findings were recorded, in §237.3. Open items are in §237.4. Hygiene:
clean.
