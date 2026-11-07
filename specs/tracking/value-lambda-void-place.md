# A value lambda and a conditional take a `void` result place — `compiler.md` §242

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`806b0196`; the contract
commit `818cb70a` changed specs only): `subscript check
a428-value-lambda-and-conditional-take-a-void-result-place.ts` gave 8
errors, each S100, then `error: 8 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 35:15 | `[1].forEach(flag ? named : K.twice)` | type mismatch: the `forEach` callback expects `(i32) or (i32, i32)` => void, got `(i32) => i32` |
| 36:15 | `[2].forEach(flag ? other : named)` | conditional branches have no common type: `(i32) => void` and `(i32) => i32` |
| 37:15 | `[3].forEach(!flag ? (x: i32): i32 => x : named)` | the `forEach` callback kind, as at 35:15 |
| 43:9 | the `m.forEach` conditional of two annotated lambdas | type mismatch: the else branch expects `(i32, string) => void`, got `(i32, string) => i32` |
| 45:13 | `each([6], (x: i32): i32 => { …; return named(x); })` | type mismatch: the argument expects `(i32) => void`, got `(i32) => i32` |
| 49:15 | `[7].forEach((x: i32): i32 => { …; return x; })` | the `forEach` callback kind |
| 53:18 | `const cb: CB = (x: i32): i32 => named(x);` | type mismatch: the initializer expects `(i32) => void`, got `(i32) => i32` |
| 55:24 | `const r: CB = flag ? (x: i32): i32 => K.twice(x) : other;` | type mismatch: the then branch expects `(i32) => void`, got `(i32) => i32` |

## After the change

`a428` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`named1 other2 named3 entrya=5 lambda6 named6 7 named8 twice9`, one per
line. `a428` has no async or generator entry, so the LIR text snapshot
does not move.

## Form

- `compiler/src/check/expr/method.rs`, the callback argument check. A
  callback place with an optional trailing index (rule 1a) keeps the
  context it had before §242: none for a function value or a
  conditional. For a conditional it also gives the leaves the place
  (`callback_leaf_place`). A callback place with no optional index,
  such as `Map.forEach`, keeps its function type as the context, so a
  conditional there checks each branch against the place with
  `require_expr_assignable`.
- `compiler/src/check/expr/operator.rs`, `check_cond`, reads
  `callback_leaf_place`: a branch that is a conditional, through
  parentheses, passes it on to its own leaves; a lambda literal leaf
  takes the place with its own number of parameters as its context;
  each other leaf is checked with no context and then takes, through
  `place_value`, the place whose arity equals the arity of its type
  (`leaf_context`, `place_callback_leaf`). The common type of the
  conditional comes after that. So a static method, a field, an
  element, a module `const`, a call result, a call of a generic
  function, and a nested conditional keep the arity of their own type.
- `compiler/src/check/expr/promise_reaction.rs`: the `finally` callback
  takes the place `() => void` as its context and through `place_value`,
  so a lambda with a value result and a conditional take it. An async
  callback keeps its S013. The witness `s186-finally-value` of the rejection total
  check had `(): i32 => 5`, which is now accepted; it now has the async
  callback `g`, which keeps the S013.
- `compiler/src/check/type_rules.rs`, `discard_lambda_result`, which
  `discard_function_result` runs first, so each place of §240 rule 2
  gets it. A lambda literal that is not async, with a result that is not
  `void` and holds no handle, whose function with the result `void` fits
  the place, gets the result `void`. `discard_return_values` makes each
  `return e;` of its body `e; return;`. The body was checked against the
  annotation before.
- An async lambda keeps the type mismatch. A `T | undefined` annotation
  keeps its S012. A result that carries a handle, such as
  `Promise<i32>[]`, keeps its diagnostic: the witness
  `s172-counted-map-callback-result` of the rejection total check has
  that form.
- One predicate, `result_can_be_discarded`, decides for a lambda and
  for a §240 reference. It is the predicate of the drop check
  (`holds_async_handle`, §188.1 rule 1), so a result that holds a
  handle at any depth (`Promise<i32>[]`, `Promise<i32>[][]`,
  `FixedArray<Promise<i32>, 1>`) is not discarded. At the pin, a §240
  reference with such a result was accepted and dropped the handle:
  `each([1], jobs)` with `jobs(x: i32): Promise<i32>[]` printed `done`,
  and `[1].forEach(jobs)` printed `one1|ok`. Now each is rejected: S100
  for the argument and the initializer, S014 for `forEach`. A mutation
  that drops the handle test from the predicate fails the unit test
  `a_result_that_holds_a_handle_keeps_its_diagnostic`.
- `compiler/tests/type_only_assignability.rs` lists the new
  `function_fits` call of `discard_lambda_result`.

## Files

- `compiler/src/check/type_rules.rs`, `compiler/src/check/expr/method.rs`,
  `compiler/src/check/expr/operator.rs`,
  `compiler/src/check/expr/promise_reaction.rs`,
  `compiler/src/check/mod.rs`, `compiler/src/check/pipeline.rs`
- `compiler/tests/value_lambda_void_place.rs`,
  `compiler/tests/type_only_assignability.rs`
- `corpus/accept/a428-value-lambda-and-conditional-take-a-void-result-place.ts`
  and `.expected`
- `generated-docs/corpus-index.md`

## Gate

One run after the implementation, and one after each Phase Review fix:

- `gate quick cc55217b dirty:11 debug 3270/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick cc55217b dirty:16 debug 3272/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 07574671 dirty:16 debug 3273/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 818cb70a dirty:16 debug 3274/0/5 skips 2 goldens-moved 0 exit 0`

The first three pins are earlier forms of the contract commit, before a
Phase Review amended it.

## Phase Review

Three MAJOR and seven MINOR findings were fixed, one MINOR finding was
recorded, and one MINOR finding needed no change, in §242.3. Open items are in §242.4. Hygiene:
clean.
