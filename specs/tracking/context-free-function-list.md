# A context-free list of functions takes the type that each fits — `compiler.md` §248

Status: IN PROGRESS.

## Red at the pin

The debug CLI built from `git archive fc3353ce` in `$TMPDIR`, run on a
`$TMPDIR` copy:

`subscript check a433-a-context-free-list-of-functions-takes-the-type-that-each-fits.ts`
gave 10 errors, then `error: 10 error(s)`:

| Line:col | Form | Message |
|---|---|---|
| 29:21 | `[none, one]` | S100 type mismatch: the array element expects `() => void`, got `(i32) => void` |
| 31:5 | `f(1)` on an element of that list | S100 `f` expects 0 argument(s), got 1: a call through a function value passes every parameter |
| 37:19 | `[LV, nv, one]`, the element `nv` | S100 type mismatch: the array element expects `() => void`, got `(i32) => i32` |
| 37:23 | the same list, the element `one` | S100, got `(i32) => void` |
| 39:5 | `h(3)` | S100 `h` expects 0 argument(s), got 1 |
| 41:45 | `[(): void => print("literal"), one]` | S100, got `(i32) => void` |
| 43:5 | `l(4)` | S100 `l` expects 0 argument(s), got 1 |
| 49:13 | `b ? none : one` | S100 conditional branches have no common type: `() => void` and `(i32) => void` |
| 51:13 | `b ? one : nv` | S100, `(i32) => void` and `(i32) => i32` |
| 53:13 | `b ? nv : one` | S100, `(i32) => i32` and `(i32) => void` |

## After the change

`a433` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`none one 1 one 2 none LV nv 3 one 3 literal one 4 none one 6 nv 7 one 5
nv 6 one 7`, one per line. `main` is not async, so the LIR text
snapshot does not change.

Each list of `a433` takes `(i32) => void`. `[LV, nv, one]` takes the
type of its third element: `LV` has fewer parameters, and `nv` has a
value result. `b ? nv : one` takes the type of its else branch.

Rule 5, measured on the controls of the unit test with the CLI of the
pin and the CLI after the change: `[one, word]`, `b ? one : word`,
`[none, work]`, `[work, one]`, `b ? none : work`, `b ? one : work`,
`[none, 1]`, and `[one, work]` (S013 of §247) give the same diagnostic.
`[(x: i32): void => {}, none]` was accepted, and stays accepted with the
same type.

## Form

- `compiler/src/check/type_rules.rs`. `place_function_list` selects the
  type: the first value whose type each other value fits. The fit is
  `leaves_fit`, the value-taking check `place_value` of §240 rule 2a,
  run on a copy for each leaf of a conditional. The copy keeps the
  lambda numbering unchanged. Each other value then takes the selected
  type through `place_leaves`, the path of §242 rule 1a, with its
  adapter. A value that is not of a function type, or a type parameter,
  gives no selection, and the present rule applies.
- `compiler/src/check/expr/aggregate.rs`. A context-free array literal
  whose first element is of a function type checks each element first,
  then selects with `place_function_list`. With no selection, each
  element takes the present check against the type of the first element
  (`place_array_element`, the present loop body).
- `compiler/src/check/expr/operator.rs`. A conditional with no context
  and no callback place uses `place_function_list` when the join of
  §146 fails.

## Open

- §248.4 item 1: a conditional element takes the first element as its
  context, so `[none, b ? one : nv]` keeps two branch diagnostics.
- §248.4 item 2: a spread element does not take part in the selection.
- §248.4 item 3: an untyped lambda parameter in a context-free array
  literal takes its type from the first element, and `tsc` rejects it.

## Files

- `compiler/src/check/type_rules.rs`,
  `compiler/src/check/expr/aggregate.rs`,
  `compiler/src/check/expr/operator.rs`
- `compiler/tests/context_free_function_list.rs` (new)
- `corpus/accept/a433-a-context-free-list-of-functions-takes-the-type-that-each-fits.ts`
  and `.expected`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s240-a-named-function-takes-a-void-result-place.md`
  (§240.4 item 5 closed)
