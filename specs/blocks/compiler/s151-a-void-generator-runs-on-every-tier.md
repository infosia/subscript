<!-- §151 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 151. A void generator runs on every tier

*(Added 2026-10-02.)* Origin: the §145 Phase Review. A defect: the tiers
disagree on a program the checker accepts.

Problem: a generator of `void` with a bare `yield;` is the coroutine
form that C8 describes (a suspension point with no value). The checker
accepts it and the dev tier runs it; the ship tier and the reference
interpreter fail. Measured at `b8d10e21` with

```ts
function* tick(n: i32): Generator<void> {
  for (let i: i32 = 0; i < n; i++) { print(`tick ${i}`); yield; }
}
```

driven by three `next()` calls (printing `done`) and a `for...of`:

| Form | Result |
|---|---|
| dev | `tick 0 / false / tick 1 / false / true / tick 0 / step` |
| ship | `subscript: internal error: type tag for Void`, exit 1 |
| interpreter (§145 review) | `invalid LIR: a generator suspension is not a yield with a value` |
| `node` | the dev output |

`tsc` 5.9.2 accepts the program. A read of `r.value` into an inferred
binding is rejected by C21 on every tier, as for any `void` value.

A bare `yield;` in a `Generator<i32>` is accepted by the checker at
`b8d10e21`; `tsc` rejects it (TS2322, "Type 'undefined' is not
assignable to type 'number'"). That is a gap in invariant 5.

### 151.1 Rules

1. A `Generator<void>` and a bare `yield;` in it lower on every tier: a
   suspension that carries no value. `.next()` returns a result whose
   `done` is read as for any generator; it has no `value` storage.
2. A `yield;` with no operand is accepted only in a generator of `void`;
   in a generator of another type it is rejected (its diagnostic names
   the declared element type), as `tsc` rejects it.
3. `for...of` over a `Generator<void>` runs its body once per
   suspension; the loop binding has type `void` and is not readable as
   a value (rule 5).
5. A `void` value is never an operand (C21, total). An expression of type
   `void` is accepted only as an expression statement. `void` is
   accepted as a type only as a return type, as the element type of
   `Generator<void>`, and as the result of `Promise<void>`. Every other
   use is rejected with S100 and C21: a binding of type `void`
   (annotated or inferred), a `void` parameter, `void` as an element or
   type argument elsewhere (`void[]`), `return f()` and `yield f()` with
   a `void` operand, a `void` argument, and a read of a `void` loop
   binding or of a `void` `value`. *(Added 2026-10-02, owner decision,
   after the Phase Review: the checker accepted these forms and the tiers
   then failed or disagreed — `yield step("a")` failed lowering on every
   tier; a `void` loop binding pushed into a `void[]` printed `0` on
   dev, `2` on the interpreter, and failed the C compile on ship.)*
4. The dev tier, the ship tier, and the reference interpreter give the
   same output for every program of rule 1; the LIR form that carries
   a value-less suspension is one form, consumed by all three.

### 151.2 Acceptance

1. Red first: an accept entry, `js-comparable: yes`, with the Problem
   program, a `Generator<void>` that returns early, one nested in a
   class method, and one driven by a `while (!r.done)` loop. At the
   contract pin it fails on the ship tier and the interpreter (record
   both outputs); its golden is the `node` output.
2. A reject entry for a bare `yield;` in a `Generator<i32>` (rule 2),
   Red at the contract pin; its header records `tsc` TS2322.
3. The new generator entry grows the LIR text snapshot; only its
   section is added. No existing `.expected` golden moves.
4. Reject entries for each rule 5 form, each with its measured `tsc`
   result; an existing accept entry that uses a rule 5 form is listed
   and changed before the rule lands.
