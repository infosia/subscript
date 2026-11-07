# An async return adopts a handle — `compiler.md` §236

Status: COMPLETE.

## Red at the pin

The debug CLI built from the code of the pin (`3aa70ee7`; the amends of
the contract changed specs only): `subscript check
a423-async-return-adopts-a-handle.ts` gave 5 errors, each S100, then
`error: 5 error(s)`:

| Line:col | Message |
|---|---|
| 37:10 | the return value expects `i32`, got `Promise<i32>` |
| 41:10 | the same |
| 65:10 | the return value expects `Mode`, got `Promise<string>` |
| 69:10 | the return value expects `i32`, got `Promise<i32>` |
| 97:58 | the same (the async arrow) |

The table is of the first version of `a423`. The forms that the Phase
Review fixes added (`try`, `using`, `for…of`, conditionals, rule 2a)
are each rejected at the pin.

## After the change

`a423` checks with no errors. The development tier, the ship tier, the
interpreter, and `node` give the same bytes, with the same order of
turns. Measured order, with a ticker that prints one line per turn:

| Form | settled handle | pending handle (two turns) |
|---|---|---|
| adoption | after `turn4` | after `turn5` |
| `return await` | after `turn3` | after `turn5` |
| held handle, two `await`s | after `turn4` | after `turn6` |
| value control | after `turn2` | |

## Form

- `compiler/src/check/async_return.rs`. An adopting return writes the
  handle local and the completion state, then returns a placeholder,
  so it leaves each scope as a `return` does. Each value return writes
  the state 0. An exception that leaves the body writes the state 0
  and is thrown again.
- The body is held in a `using` form with no binding. Its finalizer is
  the epilogue: if the state is not 0, `await [[Promise.turn]]()`,
  then `return await` of the handle of that state.
- `[[Promise.turn]]` is the async function with an empty body that the
  §186 reactions use. An `await` of it is one turn on each tier, as in
  `node`. `await Context.suspend()` gave one turn too many for a
  pending handle.
- A mixed leaf splits into `if (c) return a; else return b;`.
- The site `AsyncReturnHandle`, its divergence, its witnesses, and
  `r382` are removed. A `T extends Promise<i32>` return gets the
  general mismatch; `tsc` rejects it too (TS2322).
- The interpreter skips a null handle at `AsyncHandleRetain`, as the
  JIT and the C tier do.
- The LIR test model places a `finally` at each exit and on the
  exception path (`codegen/tests/support/lir_facts_release.rs`).

## Gate

| Round | Verdict |
|---|---|
| first | `gate quick b7268b95 dirty:21 debug 3227/1/5 skips 2 goldens-moved 1 exit 1`: a test pinned the old rejection |
| first, after that test changed | `gate quick b7268b95 dirty:22 debug 3228/0/5 skips 2 goldens-moved 1 exit 0` |
| epilogue | `gate quick a85d3da8 dirty:31 debug 3228/1/5 skips 2 goldens-moved 1 exit 1`: C8 cited `r382` |
| mixed leaf | `gate quick 5a5f27c1 dirty:32 debug 3229/0/5 skips 2 goldens-moved 1 exit 0` |
| completion state | `gate quick 30c3d388 dirty:33 debug 3230/0/5 skips 2 goldens-moved 1 exit 0` |
| last MINOR fixes, stable tree | `gate quick f6033e60 dirty:37 debug 3231/0/5 skips 2 goldens-moved 1 exit 0` |

Each moved golden is the LIR text snapshot, which grows by the `a423`
block only.

## Phase Review

Three MAJOR and several MINOR findings, each fixed, recorded in §236.3.
Open items in §236.4. Hygiene: clean.
