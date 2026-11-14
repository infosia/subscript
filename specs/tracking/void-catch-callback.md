# Tracking: a `void` `catch` callback gives a `Promise<void>` (§289)

Contract: `specs/blocks/compiler/s289-a-void-catch-callback-gives-a-promise-of-void.md`.
The owner selected §238.4 item 4 on 2026-11-14. The rule replaces the
`catch` part of the owner decision of 2026-10-09 in §186.

## Red

At the contract commit `449dde37`, `subscript check` of
`a454-a-void-catch-callback-gives-a-promise-of-void` exited 1 with eight
S013 "a `catch` callback must return `i32`; a `void` callback is
accepted only on `Promise<void>`" (`string` and `Box` for those
receivers), one at each `void` `catch` callback.

## Form

- `compiler/src/check/expr/promise_reaction.rs`: a `catch` callback
  with `Delivery::Void` is accepted on each `Promise<T>`, and the call
  has the result `Promise<void>`.
- `compiler/src/check/expr/promise_reaction/helper.rs`: the §186 helper
  takes a key for this shape. The helper awaits `h`, drops its value,
  and calls the callback on failure.
- The rejection witness `s186-catch-void` is now accepted. The site
  `PromiseReactionResult` takes the witness `s289-then-rejection-void`,
  a `void` rejection callback of `then(f, r)`, which rule 4 keeps
  rejected.
- Rule 1a: an expression statement records the span of the call that
  is the direct operand of its `await` (`void_catch_statement`). A
  `void` `catch` call with another span gives S013 "a `void` `catch`
  callback on `Promise<i32>` gives `Promise<i32 | void>`; use it only
  as `await h.catch(r);`".
- `Promise.all` reads the async origins of its jobs before it checks
  their type, so a rejected element gives no second S013.
- `a454` output: 16 lines, equal under the coding agent's hand trace,
  `node`, and the CLI.

## Phase Review

One fresh review: one MAJOR and four MINOR findings, each fixed; see
§289.3. The MAJOR program gives three S013 of rule 1a, at 4:47, 7:22,
and 8:32; `await (wi().catch(v));` is accepted.

## Gate

- `gate full ff191065 dirty:18 debug 3420/2/5 release 3401/2/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 1`.
  Two tests failed: `contextual_result_inference.rs`
  `an_async_reaction_callback_gives_a_string_literal_the_type_string`
  and `nested_then_context.rs`
  `a_nested_then_in_a_rejection_callback_takes_the_context`. The fix of
  review finding 2 named `Promise<…>` for each adopted callback result.
  It now names `Promise<void>` only for an adopted `void` value; each
  other adopted result names its value, as at the pin.
- `gate full ff191065 dirty:18 debug 3422/0/5 release 3403/0/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a454`
  section only.

## §292: an async `void` `catch` callback gives a `Promise<void>`

Contract: `specs/blocks/compiler/s292-an-async-void-catch-callback-gives-a-promise-of-void.md`.

### Red

At the contract commit `3b6c0853`, `subscript check` of `a456` gave six
S013 "a `catch` callback must return `i32`, the value type of the
handle; it returns `Promise<void>`", at lines 12, 13, 14, 16, 26, and 38.

### Form

- `compiler/src/check/expr/promise_reaction.rs`: a `catch` callback
  whose adopted result is `void` takes the §289 path, with the §186
  adoption helper.
- `a456` output, by hand and under `node`, 19 lines, from `bad` to
  `second done`.

### Phase Review

One fresh review. Four MINOR findings, each fixed; see §292.3.

### Gate

- The first full gate failed in `apparent_type_shapes`: two guards in
  `check_promise_reaction` compared a type in a variant guard. One
  helper `callback_gives_void` now reads the apparent type for both.
- `gate full 636fcf97 dirty:10 debug 3438/0/5 release 3419/0/5 skips 1/0 clippy 4/10/13 goldens-moved 1 exit 0`.
  The moved golden is the LIR text snapshot, which grows by the `a456`
  section only.
