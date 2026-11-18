<!-- §334 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 334. An interface method, a spread, and a `then` callback take handle variance

*(Added 2026-11-18.)* Origin: §235.4 items 2 and 3, rank 13 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `3e30f5fc` with `tsc` 5.9.2. `class C` is a
reference class. §235 rule 3 relates two function types by the "fits"
relation of §235 rule 2: a result `C` fits `C | null`, and a parameter
`C | null` takes a `C`. These places still require the same type:

| Form | subscript | `tsc` |
|---|---|---|
| `interface I { m(): C \| null; }`, `class K implements I { m(): C { … } }` | S100 "class `K` implements `m` of interface `I` with type `() => C`; the interface requires `() => C \| null`" | accepts |
| the same with `string` in place of `C` | S100 | accepts |
| `interface I { m(x: C): i32; }`, `class K { m(x: C \| null): i32 }` | S100 | accepts |
| `const gs: ((x: C) => C)[] = [id]; const fs: ((x: C) => C \| null)[] = [...gs];` | S100 "the array element expects `(C) => C \| null`, got `(C) => C`" | accepts |
| `await mk().then((x: C \| null): i32 => 1)` with `mk(): Promise<C>` | S100 "a `.then(...)` callback parameter must be `C`" | accepts |

`Array.from<F>(ids)` and the generic inference of §235.4 item 3 are
accepted at the pin. A `Set` of function values is S014 (Q24: a
function type is not a key kind), which this section does not change.

### 334.1 Rules

1. **An interface method.** A class method implements an interface
   method if the function type of the class method is assignable to
   the function type of the interface method by §235 rule 3. So the
   class result fits the interface result, and each interface parameter
   fits the class parameter. A narrower class parameter
   (`m(x: C)` for `m(x: C | null)`) stays rejected, although `tsc`
   accepts it by method parameter bivariance: a call through the
   interface can pass `null`.
2. **A spread element.** A spread of an array of `S` into an array
   literal whose element type is `T` is accepted if `S` fits `T`
   (§235 rule 2). The spread copies each element into a new array, so
   no location holds two element types (C17 does not apply).
3. **A `then` callback parameter.** A `then` callback parameter of type
   `P` is accepted if the value type of the handle fits `P`.
4. **No conversion.** §235 rule 5 holds: the value of `T` and of
   `T | null` have one representation, so no code changes for the
   value. §235 rule 5a records a conversion where the checker accepts
   two function types that are not identical, a spread element
   included.
5. **Other places stay.** An interface field, an array that is not a
   spread copy (`const ys: (C | null)[] = xs`), and a `Map` or `Set`
   element type keep their rule.

### 334.2 Acceptance

1. Red first: an accept entry with the three accepted interface forms
   of the problem table, each called through the interface and on the
   class; the spread; and the `then` callback. It prints each result. It
   is `js-comparable: yes` if `node` prints the same bytes. At the pin,
   `check` gives S100.
2. A reject entry for the narrower class parameter of rule 1, with its
   text. Its header states what `tsc` does, measured.
3. A unit test for rules 1 to 3, each with a control in the same shape
   that stays rejected (core principle 9).
4. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after.

### 334.3 Sections this one amends

- §235 rule 4: an interface implementation, a spread element, and a
  `then` callback parameter are places of rule 3. §235.4 items 2 and 3
  close.
