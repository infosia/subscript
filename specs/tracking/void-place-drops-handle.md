# A `void` result place names the handle that it drops — `compiler.md` §247

Status: IN PROGRESS.

## Red at the pin

The debug CLI built from the code of the contract commit `4582d8fe`, in
`$TMPDIR`:

`subscript check r496-a-void-result-place-names-the-handle-it-drops.ts`
gave one S100 "type mismatch: the argument expects `(i32) => void`,
got `(i32) => Promise<void>`" at 21:16, why C24.

The forms of the table, and two conditionals, with the debug CLI built
from the same code (`b9bef6f9`; the commits after it changed specs
only):

| Form | At the pin |
|---|---|
| `each([1], work);` | S100 the argument kind, why C24 |
| `const f: CB = work;` | S100 the initializer kind |
| `each([1], val);` | S100, got `(i32) => Promise<i32>` |
| `each([1], async (x: i32): Promise<void> => { … });` | S100, the argument kind |
| `const w = work; each([1], w);` | S100, the argument kind |
| `each([1], (x: i32) => work(x));` | S013 "an async handle is dropped without any await of its completion" |
| `[1].forEach(work);` | S014 "`forEach` cannot carry a counted callback result (§171)" |
| `[1].forEach(async (x: i32): Promise<void> => {});` | S014, the same |
| `each([1], b ? work : work);` | two S100, "the then branch" and "the else branch" |
| `[1].forEach(b ? work : (x: i32): void => {});` | S100 "conditional branches have no common type" |

## After the change

`r496` gives one S013 "function value `work` returns `Promise<void>`,
which the place `(i32) => void` drops, and no holder awaits the handle;
a place with the result `Promise<void>` can await it" at 21:16, why
§70. `tsc` 5.9.2 accepts `r496`.

Each form of the table except the lambda that calls `work` gives S013
with that text. A built-in callback puts "`forEach` callback" before
it, and so does a leaf of a callback conditional. A lambda literal has
no name in the text. The lambda that calls `work` keeps the S013 of the
drop check. Each leaf of a conditional that holds the handle gets the
text of rule 2: two S013 for `b ? work : work`, and one for the
`forEach` conditional.

A result that contains the error type gets no S013: with
`async function* ag(x: i32): AsyncGenerator<i32>`, `each([1], ag)`
keeps the S100 "got `(i32) => Promise<<error>>`" after the diagnostics
of the declaration.

## Form

- `compiler/src/check/type_rules.rs`. `dropped_handle` decides rule 1
  with the decision of the adapter. The place is a function type with
  the result `void`, and the value is a function whose result holds an
  async handle (`holds_async_handle`, §242 rule 5) and no error type
  (`interface::poisoned`). The value takes `adapter_shape` with the
  result that the place drops, and the adapted type must fit the place
  by `assignable`. A conditional is not a value for it; each leaf is.
  `dropped_handle_message` gives the text of rule 2: the name of the
  value when it has one, the result type, and the place type.
- The facts of the callee come from one source, `callee_facts`: those
  of `adapter_callee`, or those of a const local name with the type of
  the value. `refused` is a generic or generator value; `is_async` is
  an async function or a value with a handle result. `adapter_shape`
  refuses `is_async` except when the place drops the handle. Its fit is
  `prefix_parameters_fit` and `result_fits`, the two halves of
  `prefix_fits`; with a dropped handle the result fits a `void` place.
  The result of the value is the one of its type, because the signature
  of an async function records the awaited result.
- `CallbackLeaves` carries the method, so a leaf of a callback
  conditional names it as the callback does.
- `require_expr_assignable` gives the diagnostic before the §246 rule 5
  check. `place_leaves` gives it to a callback leaf and makes the
  conditional `Error`, so no "no common type" follows.
  `expect_callback_shape` gives it before the counted check of §171.
- The new site `VoidPlaceDropsAsyncHandle` (S013) has the divergence
  `DroppedAsyncHandle` of the present S013 (§70). Its witnesses are
  `s247-argument` and `s247-callback`; `tsc` 5.9.2 accepts both forms.
- The witness `s172-counted-map-callback-result` had a `Map.forEach`
  result `Promise<i32>[]`, which now reaches the new site. It is now
  `s171-counted-map-groupby-result`, tag §171, with a `Map.groupBy` key
  result `Promise<i32>[]`. That place reads the result, so it keeps
  §171.
- Rule 4: a value whose parameters do not fit keeps its present
  diagnostic, and a counted result in a place that reads it (`map`)
  keeps §171. The unit test pins both. In `forEach`, an async lambda
  with parameters that do not fit keeps the §171 diagnostic that it
  had at the pin.
- The unit tests of §240, §242, §243, §244, §246, and the counted
  `Map.forEach` result pinned the former S100 or S014 for a handle
  result in a `void` place. They now pin the S013 of rule 2.

## Files

- `compiler/src/check/type_rules.rs`,
  `compiler/src/check/expr/callback_shape.rs`,
  `compiler/src/check/expr/method.rs`, `compiler/src/check/interface.rs`
- `compiler/src/check/rejection_sites.rs`,
  `compiler/src/check/rejection_surface_classes.rs`,
  `compiler/src/check/rejection_witness_index.rs`,
  `compiler/src/check/rejection_witnesses.txt`
- `compiler/tests/void_place_drops_handle.rs` (new),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/type_only_assignability.rs`,
  `compiler/tests/built_in_callback_fewer_parameters.rs`,
  `compiler/tests/const_local_function_adapter.rs`,
  `compiler/tests/fewer_parameters_function_type.rs`,
  `compiler/tests/lambda_void_result.rs`,
  `compiler/tests/named_function_void_place.rs`,
  `compiler/tests/value_lambda_void_place.rs`
- `corpus/reject/r496-a-void-result-place-names-the-handle-it-drops.ts`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s240-a-named-function-takes-a-void-result-place.md`
  (§240.4 item 2 closed),
  `specs/blocks/compiler/s247-a-void-result-place-names-the-handle-it-drops.md`
  (§247.3 and §247.4)

## Phase Review

One review: no CRITICAL, no MAJOR. Five MINOR findings are fixed, and
one contrived MINOR is recorded in §247.4. The fixes: the method name
of a callback leaf, one source for the refusal of the adapter, no S013
for a poisoned result, the §171 witness name, and two test names.
