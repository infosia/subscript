# A built-in callback takes a function with fewer parameters — `compiler.md` §244

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`6f9a38c3`; the contract
commit `b1db79c0` changed specs only): `subscript check
a430-built-in-callback-takes-a-function-with-fewer-parameters.ts` gave
11 errors, then `error: 11 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 28:18 | `[1, 2].forEach(() => { … })` | S014 `forEach` callbacks take 1 parameter(s), or 2 with a trailing `i32` index; got 0 (Q27) |
| 32:15 | `[1].forEach(none)` | the same |
| 33:23 | `[1, 2].map((): i32 => 0)` | S014, the `map` kind |
| 34:29 | `[1, 2, 3].filter((): boolean => true)` | S014, the `filter` kind |
| 35:13 | `m.forEach((v: i32) => …)` | S014 `Map.forEach` callbacks take exactly 2 parameter(s); extra lib callback parameters are not accepted (Q24) |
| 36:13 | `m.forEach(() => …)` | the same |
| 37:13 | `s.forEach(() => …)` | S014 `Set.forEach` callbacks take exactly 1 parameter(s); … (Q24) |
| 38:26 | `[1, 2].reduce((acc: i32): i32 => acc + 1, 0)` | S014 `reduce` callbacks take 2 parameter(s), or 3 with a trailing `i32` index; got 1 (Q27) |
| 39:15 | `[6].forEach(b ? two : (x: i32): void => {})` | S100 conditional branches have no common type: `(i32, i32) => void` and `(i32) => void` |
| 40:15 | `[7].forEach(b ? one : none)` | S100 conditional branches have no common type: `(i32) => void` and `() => void` |
| 42:17 | `fixed.forEach(() => { … })` on a `FixedArray` | S014, the `forEach` kind |

## After the change

`a430` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`count=2 none 0,0 3 v1 v2 m m s 2 two6@0 one7 count=5`, one per line.
`a430` has no async or generator entry, so the LIR text snapshot does
not move.

## Form

- `compiler/src/check/expr/callback_shape.rs` holds the arity and the
  shape of a built-in callback, moved from `method.rs`.
  `callback_params_for_arity` gives a lambda literal with fewer
  parameters the list of the smallest arity, as its context.
  `expect_callback_shape` first gives a value with fewer parameters the
  place of the smallest arity, through the §243 conversion
  (`discard_function_result`): a lambda
  literal is padded, and a reference of §243 rule 2 becomes a lambda with
  no environment. A function value with an environment, and a function
  with a default parameter, keep the arity diagnostic.
- `compiler/src/check/type_rules.rs`, `place_callback_leaves` (rule 2).
  The outer conditional of a callback place places its leaves after all
  of them are checked: it reads the arity of each leaf from the HIR,
  takes the largest that the place allows, and gives each leaf that
  place. A nested conditional of the callback place takes the type of the
  place when its two branches have it, and gives the diagnostic of no
  common type when they do not. A leaf with more parameters than the
  place allows keeps its type, so its conditional has no common type.
- Rule 4a: a function value with fewer parameters that rule 1 does not
  take has its own site, `CallbackFewerParametersUnadapted` (S014), with
  the divergence `CallbackFewerParametersUnadapted`. Its message names
  the cause: "`forEach` callback `f` is a function value with fewer
  parameters than the place; an adapter for it needs a new environment,
  which no rule gives an owner; use a lambda that calls it", or
  "`reduce` callback `d` has fewer parameters than the place and a
  default parameter, which has no adapter; use a lambda that calls it".
  The witnesses `s244-environment` and `s244-default` have the two
  forms; `tsc` 5.9.2 accepts both, and `node` prints `e` and `0`. Any
  other value with fewer parameters that rule 1 does not take, such as a
  lambda literal or a reference whose types do not fit, or a
  conditional, gets the type mismatch of the place or of its branches.
  Only a value with more parameters than the place allows reaches
  `CallbackParameterCount`. The cells of the generic `tsc` matrix with a
  function value of no parameters in a `FixedArray` or `Map.groupBy`
  callback now name the record `compiler.md §244`.
- The divergence `CallbackParameterShape` and the witness `s038-d` of
  the rejection total check had `xs.map((): i32 => 1)`, which is now
  accepted. Each now has the `Map.forEach` form with a third parameter,
  and the divergence cites `stdlib.md §10.4`.
- The §242 and §243 unit tests that pinned the rejection of fewer
  parameters in a built-in callback now accept those forms, and keep a
  control with more parameters than the place allows.
- With the padding in `expect_callback_shape`, each built-in callback
  place already has the arity of its value, so the parameter count of
  §243 (`ParameterCount`) changed no result and is removed. The
  conversion of a function value to its place has one form for every
  place.

## Files

- `compiler/src/check/expr/callback_shape.rs` (new),
  `compiler/src/check/expr/method.rs`, `compiler/src/check/expr.rs`,
  `compiler/src/check/expr/operator.rs`, `compiler/src/check/type_rules.rs`,
  `compiler/src/check/mod.rs`
- `compiler/src/check/rejection_witnesses.txt`,
  `compiler/src/divergence/established_tail.rs`
- `compiler/tests/built_in_callback_fewer_parameters.rs` (new),
  `compiler/tests/fewer_parameters_function_type.rs`,
  `compiler/tests/value_lambda_void_place.rs`,
  `compiler/tests/type_only_assignability.rs`
- `corpus/accept/a430-built-in-callback-takes-a-function-with-fewer-parameters.ts`
  and `.expected`
- `generated-docs/corpus-index.md`

## Gate

One run after the implementation, and one after each Phase Review fix:

- `gate quick 212c4584 dirty:17 debug 3282/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 94dae3c1 dirty:23 debug 3283/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick b1db79c0 dirty:24 debug 3284/0/5 skips 2 goldens-moved 0 exit 0`

The first two pins are earlier forms of the contract commit, before a
Phase Review amended it.

## Phase Review

Two MAJOR and two MINOR findings were fixed, and one MINOR finding was
recorded, in §244.3. Open items are in §244.4. Hygiene: clean.
