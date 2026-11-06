# Each call reads its type arguments — `compiler.md` §231

Status: COMPLETE.

## Red at the pin `22f44fef`

The debug CLI built from the checker of the pin:

- `subscript check r488-user-call-unread-type-argument.ts`: `no errors`
- `subscript check r489-reduce-block-literal-result.ts`: `no errors`

## After the change

- `r488`: S100 "method `get` is not generic" at 15:15. `tsc` gives
  TS2558 at 15:16.
- `r489`: S100 at 12:12, the §230 rule 1 text. `tsc` gives TS2322 at
  11:3 and TS2769 at 11:20.

| Form | subscript | `tsc` |
|---|---|---|
| `b.get<i32>()` | S100 "method `get` is not generic" | TS2558 |
| `f<i32>(1)`, a function value | S100 "`f` is not generic" | TS2558 |
| `Reg.f<i32>(1)` | S100 "`f` is not generic" | TS2558 |
| `g<i32>()` | S100 "`g` is not generic", as at the pin | TS2558 |
| `B.s<i32>()` | S100 "static method `B.s` is not generic", as at the pin | TS2558 |
| `id<i32, i32>(1)` | S100 "`id` expects 1 type argument(s), got 2", as at the pin | TS2558 |
| `await new B().load<i32>()` | S100 "method `load` is not generic", as at the pin | TS2558 |
| a block that returns `"b"` only, `reduce` and `reduceRight` | rejected | TS2322, TS2769 |
| a block that returns `"b"` on two paths | rejected | TS2322, TS2769 |
| a block that returns `"b"` or `acc` | accepted | accepts |
| a block that returns `"b"` or `"a"` | accepted | accepts |
| a block with `(acc: Mode, x)` (rule 1a) | accepted | accepts |
| a block with a `: Mode` result | accepted | accepts |

## Form

- The user-call exemptions of §230 are now `mark_user_call`. The mark
  selects the text of the diagnostic only. A user call runs the same
  check as a built-in call, through `checked_call`.
- The site of a user call is `NonGenericFunctionTypeArguments`.
- `reject_widened_reduce_result` reads the expression body, or each
  `return` value of a block. It does not enter a nested function.
- A receiver whose apparent type is an interface is a user receiver. A
  type parameter with a built-in apparent type keeps the built-in text.
- A callee with no name reads "the callee is not generic".

## Gate

The first quick gate ran while the Phase Review fixes changed the tree,
so its verdict is not used. The run after the fixes:

`gate quick 22f44fef dirty:16 debug 3197/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

Four MINOR findings, each fixed, recorded in §231.3. Hygiene: clean.
