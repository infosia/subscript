# A `then` callback result takes the context of the call — `compiler.md` §233

Status: COMPLETE.

## Red at the pin `21bc2228`

The debug CLI built from the code of the pin (`b58ce57d`):
`subscript check a420-then-result-takes-context.ts` gave 4 errors, each
S100, then `error: 4 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 29:10 | `literal` | the return value expects `Promise<Mode>`, got `Promise<string>` |
| 33:32 | `buffer` | cannot infer type parameter `T` of `empty`: no candidate |
| 37:10 | `both`, `then(f, r)` | the return value expects `Promise<Mode>`, got `Promise<string>` |
| 41:19 | `awaited` | the initializer expects `Mode`, got `string` |

## After the change

`a420` checks with no errors. The development tier prints:

```
literal:a
buffer:0
both:a:b
awaited:b
```

The ship tier, the interpreter, and `node` give the same bytes.

## Form

- `reaction_result_context` in `compiler/src/check/inference.rs` gives
  `C` if the active call is this `then` call and its context is
  `Promise<C>`.
- `check_reaction_arguments` in
  `compiler/src/check/expr/promise_reaction.rs` gives `C` to an arrow
  fulfilment callback as its result hint (§229). The rejection callback
  keeps §186.1 rule 4.
- Rule 2 needed no added code: the awaited call already sets the
  context `Promise<C>` before it checks `h.then(…)`.
- `a420` has an async entry, so the LIR text snapshot grows by its block
  only.
- Rule 1a: `check_reaction_callback` drops the hint for an async arrow
  whose result is a string literal, for each callback role.

## Rule 1a, measured

`tsc` 5.9.2 and the debug CLI. `v(n): Promise<i32>`, `m(): Promise<Mode>`,
`w64(): Promise<i64>`.

| Form | Context | Before rule 1a | After | `tsc` |
|---|---|---|---|---|
| `v(n).then(async (x: i32) => "a")` | `Promise<Mode>` | accepted | S100 got `Promise<string>` | TS2322 |
| `await v(1).then(async (x: i32) => "b")` | `Mode` | accepted | S100 got `string` | TS2322 |
| `m().catch(async (e: Error) => "b")` | `Promise<Mode>` | accepted | S013, the callback returns `string` | TS2322 |
| `v(1).then((x: i32) => "a", async (e: Error) => "b")` | `Promise<Mode>` | accepted | S013, the callback returns `string` | TS2322 |
| `w64().catch(async (e: Error) => 0)` | `Promise<i64>` | accepted | accepted | accepts |
| `v(1).then(async (x: i32) => 1)` | `Promise<i64>` | accepted | accepted | accepts |

"Before rule 1a" is the §233 tree before the Phase Review fix. At the pin
the first two rows were rejected, and the `catch` rows were accepted. No
corpus entry or example has an async `then` or `catch` callback.

## Gate

The first quick gate ran while the Phase Review fixes changed the tree,
so its verdict is not used. The run after the fixes:

`gate quick a97c4781 dirty:9 debug 3206/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a420`
block.

## Phase Review

One MAJOR and four MINOR findings, recorded in §233.3. Hygiene: clean.
