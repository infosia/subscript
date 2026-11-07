# A nested then takes the context of its adoption — `compiler.md` §245

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`ffa2f1f0`; the contract
commit `fbc43292` changed specs only): `subscript check
a431-nested-then-takes-the-context-of-its-adoption.ts` gave 3 errors,
each S100, then `error: 3 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 15:10 | `return v(1).then((x: i32) => v(x).then((y: i32) => "a"));` in `h2(): Promise<Mode>` | type mismatch: the return value expects `Mode`, got `Promise<string>` |
| 19:10 | the same with three levels, in `h3(): Promise<Mode>` | the same |
| 25:28 | `const p: Promise<Mode> = v(20).then((x: i32) => v(x).then((y: i32) => "b"));` | type mismatch: the initializer expects `Promise<Mode>`, got `Promise<string>` |

## After the change

`a431` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`v1 v2 h2=a v20 v21 p=b v10 v11 v12 h3=b bad1 v30 r=b badMode v40 c=a`,
one per line. `main` is
async, so the LIR text snapshot grows by the `a431` block only.

## Form

- `compiler/src/check/expr/promise_reaction.rs`,
  `check_reaction_callback`. When a fulfillment callback is an arrow
  whose expression body, through parentheses, is a `then` call
  (`is_then_call`), its result hint is `Promise<C>` in place of `C`.
  The body is checked with that context, so the inner call has the
  context `Promise<C>`, and §233.1 rule 1 gives its callback the
  context `C`. The rule applies again at each level. A block body and a
  body that is not a `then` call keep the hint `C`. The hint gives a
  context only; a body result that is not a literal keeps its type.
- The same hint holds for each callback with a result context: the
  rejection callback of `then(f, r)` and the callback of `catch`. `a431`
  has both. At the pin, each gave S013 "a rejection callback of
  `.then(...)` must return `Mode`, the value of its fulfillment
  callback; it returns `string`" and "a `catch` callback must return
  `Mode`, the value type of the handle; it returns `string`", measured
  with a CLI built from the pin.

## Files

- `compiler/src/check/expr/promise_reaction.rs`
- `compiler/tests/nested_then_context.rs`
- `corpus/accept/a431-nested-then-takes-the-context-of-its-adoption.ts`
  and `.expected`
- `codegen/tests/lir-goldens/corpus.txt` (the `a431` block)
- `generated-docs/corpus-index.md`

## Gate

One run after the implementation, and one after the Phase Review fix:

- `gate quick c5231e29 dirty:8 debug 3286/0/5 skips 2 goldens-moved 1 exit 0`
- `gate quick fbc43292 dirty:9 debug 3287/0/5 skips 2 goldens-moved 1 exit 0`

The first pin is an earlier form of the contract commit, before the
Phase Review amended it. In each run the moved golden is the LIR text
snapshot, which grows by the `a431` block only: the `main` of `a431` is
async.

## Phase Review

One MAJOR and two MINOR findings were fixed, and one MINOR finding was
recorded, in §245.3. Open items are in §245.4. Hygiene: clean.
