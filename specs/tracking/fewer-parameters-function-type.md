# A function with fewer parameters takes a function type — `compiler.md` §243

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`93daadf4`; the contract
commit `1d1b9a61` changed specs only): `subscript check
a429-function-with-fewer-parameters-takes-a-function-type.ts` gave 10
errors, each S100, then `error: 10 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 35:14 | `eachi([1], (x: i32): void => …)` | type mismatch: the argument expects `(i32, i32) => void`, got `(i32) => void` |
| 36:14 | `eachi([2], (x) => …)` | the same |
| 37:14 | `eachi([3], () => …)` | the same, got `() => void` |
| 38:14 | `eachi([4], one)` | the same, got `(i32) => void` |
| 39:16 | `eachi([5], K.one)` | the same |
| 40:14 | `eachi([1], twice)` | the same, got `(i32) => i32` |
| 41:18 | `const f: CB2 = one;` | type mismatch: the initializer expects `(i32, i32) => void`, got `(i32) => void` |
| 43:17 | `const t: F2 = twice;` | the initializer kind, `(i32, i32) => i32` against `(i32) => i32` |
| 45:17 | `const r: F2 = (x: i32): i32 => x + 1;` | the same |
| 47:18 | `const g: CB2 = (x: i32): void => …` with a capture | the initializer kind |

## After the change

`a429` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`l1 u2 z one4 K.one5 twice1 one6 twice7 t=14 r=9 19`, one per line.
`a429` has no async or generator entry, so the LIR text snapshot does
not move.

## Form

- `compiler/src/check/type_rules.rs`. `place_function_value` and
  `place_value` take a `ParameterCount`: `Fewer` for the places of §240
  rule 2, `Same` for a built-in callback (`place_callback_value`, the
  leaves of §242 rule 1a, and `expect_callback_shape`).
- `pad_lambda_parameters` (rule 1): a lambda literal with fewer
  parameters, whose parameters and result fit the first parameters and
  the result of the place (`prefix_fits`), takes the trailing parameters
  of the place. The lambda keeps its own parameter types, and its type is
  its own parameters and the trailing parameters of the place; a place
  of other types that fit adds the cast of §235 rule 5a, as for a
  reference. Then `discard_lambda_result` of §242 runs.
- `adapt_function_reference` (rule 2) is the one builder of the lambda
  with no environment, for §240 and §243. It takes the parameters of the
  place, calls the function with its own number of arguments, and
  discards the result when the place has the result `void` and the
  function a value result. With fewer parameters, a function with a
  default parameter, a generic function, and an async function are not
  converted.
- A unit test pins rule 3 for a reference in a built-in callback
  (`forEach`, a conditional, `reduce`, `Set.forEach`). A mutation of the
  three `ParameterCount::Same` uses to `Fewer` fails it.
- `compiler/tests/type_only_assignability.rs` lists `prefix_fits` and
  the renamed `place_value_with`.

## Files

- `compiler/src/check/type_rules.rs`, `compiler/src/check/expr/method.rs`
- `compiler/tests/fewer_parameters_function_type.rs`,
  `compiler/tests/type_only_assignability.rs`
- `corpus/accept/a429-function-with-fewer-parameters-takes-a-function-type.ts`
  and `.expected`
- `generated-docs/corpus-index.md`

## Gate

One run after the implementation, and one after the Phase Review fix:

- `gate quick 1d1b9a61 dirty:8 debug 3277/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 1d1b9a61 dirty:10 debug 3279/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

Two MINOR findings were fixed, and one MINOR finding was recorded, in
§243.3. Open items are in §243.4. Hygiene: clean.
