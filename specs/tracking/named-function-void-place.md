# A named function takes a `void` result place — `compiler.md` §240

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code base `7d4178ac` (the contract pin
`cf04db36` changed specs only): `subscript check
a427-named-function-takes-a-void-result-place.ts` gave 11 errors, each
S100, then `error: 11 error(s)`:

| Line:col | Place | Message |
|---|---|---|
| 35:25 | field initializer `g: (x: i32) => void = named` | type mismatch: the field initializer expects `(i32) => void`, got `(i32) => i32` |
| 49:10 | `return named;` from `pick(): (x: i32) => void` | type mismatch: the return value expects `(i32) => void`, got `(i32) => i32` |
| 54:14 | `xs.forEach(named)` | type mismatch: the `forEach` callback expects `(i32) or (i32, i32)` => void, got `(i32) => i32` |
| 55:14 | `xs.forEach(two)` | the same, got `(i32, i32) => boolean` |
| 56:12 | `each(xs, named)` | type mismatch: the argument expects `(i32) => void`, got `(i32) => i32` |
| 57:14 | `each(xs, K.twice)` | the same |
| 58:32 | `const cb: (x: i32) => void = named;` | type mismatch: the initializer expects `(i32) => void`, got `(i32) => i32` |
| 63:13 | `m.forEach(entry)` | type mismatch: the `Map.forEach` callback expects `(string, string)` => void, got `(string, string) => i32` |
| 67:13 | `s.forEach(member)` | type mismatch: the `Set.forEach` callback expects `(string)` => void, got `(string) => string` |
| 68:29 | `new Holder(named)` | type mismatch: the argument expects `(i32) => void`, got `(i32) => i32` |
| 70:16 | `holder.f = K.twice;` | type mismatch: the assignment expects `(i32) => void`, got `(i32) => i32` |

## After the change

`a427` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes.
`main` of `a427` is async (it awaits a `finally` reaction), so the LIR
text snapshot grows by the `a427` block.

The emitted C of `xs.forEach(named)` and of
`xs.forEach((x: i32) => { named(x); })` is the same, except the path of
the source file in the position table.

## Form

- `compiler/src/check/type_rules.rs`, `discard_function_result`. A
  `FuncRef` of a function that is not generic, not async, and not a
  generator, with a result that is not `void` and not a handle, in a
  place of a function type with the result `void`, becomes a lambda
  with no environment when the function with the result `void` fits
  the place. Its body calls the function with the parameters of the
  lambda and discards the result.
- `place_function_value` runs it first, so each §235 rule 5a place gets
  it. The built-in callback check (`expect_callback_shape`) runs it with
  the place type of the callback, before the counted-result check.
- A function value with an environment, an async function, and a
  generic function keep their diagnostic.
- Rule 2a: `place_value` is the check that takes the value with no
  diagnostic: the conversion of the place, then the type check.
  `require_expr_assignable`, `??=` (`nullish_assignment_type`), `||=`
  and `&&=` (`check_logical_boolean_value`), and `??`
  (`nullish_result_type`) decide with it. Before this, those three
  sites decided with `assignable` first: `handler ??= named` printed
  `no handler`, `f &&= named` kept the old function, and
  `const f = n ?? named` stopped the lowering ("expected scalar, got
  Pair"). Each now equals `node`. `||=` and `&&=` on a function place
  give the boolean-operand diagnostic of §199.1 rule 6, as they do
  with a `void` function on the right.
- `compiler/tests/type_only_assignability.rs` scans for `assignable`
  calls too. Each of the 15 remaining calls is listed by file and
  function with the reason that no conversion applies.
- An element of an array literal beside a spread takes
  `require_expr_assignable`; only the spread source keeps
  `require_assignable` on its element type. `[...base, named]` and
  `[...base, nb]` with `nb(b: B | null): void` are accepted.
- A `finally` callback that is not an arrow takes `place_value` with
  the place `() => void`, so `one().finally(tick)` with
  `tick(): boolean` prints `one`, `tick`, `1`, as `node` does. An async
  callback keeps its S013.
- The total check also scans `type_fits` and `function_fits`; each of
  the 8 calls in 6 functions has its reason.
- A function with a result `T | undefined` keeps its C7 diagnostic.
- `a427` adds `handler ??= named`, `none ?? K.twice`,
  `[...base, named]`, and `one().finally(tick)`; its golden gains
  `named8`, `twice9`, `twice10`, `named10`, `one`, `tick`, and `1`,
  which equal `node`.

## Gate

One run after the implementation, and one after each Phase Review fix:

- `gate quick 5e50208f dirty:9 debug 3251/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 22c2d271 dirty:12 debug 3255/0/5 skips 2 goldens-moved 0 exit 0`
- `gate quick 8db556f8 dirty:15 debug 3258/0/5 skips 2 goldens-moved 1 exit 0`

In the last run the moved golden is the LIR text snapshot, which grows
by the `a427` block only: the `main` of `a427` is async.

## Phase Review

One CRITICAL, two MAJOR, and two MINOR findings were fixed, and the
others were recorded, in §240.3. Open items are in §240.4. Hygiene:
clean.
