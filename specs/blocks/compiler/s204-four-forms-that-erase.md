<!-- §204 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 204. Global `isNaN` and `isFinite`, an array in a template, `satisfies`, and `debugger`

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected this group as the fourth section of the second
group. The measurement is in `specs/tracking/trivial-erasures.md`.

Problem, measured at the pin `c258a296` (`tsc` 5.9.2 accepts each row, and
`node` runs it):

| Form | `subscript check` | Recorded reason |
|---|---|---|
| `isNaN(x)`, `isFinite(x)` | S014 "The global form coerces its argument; use `Number.isNaN`" | `stdlib.md` §11.1, Q25 |
| `` `${xs}` `` with `xs: i32[]` | S100 "type `i32[]` cannot be interpolated into a template" | Q14, C24 row 22 |
| `x satisfies T` | S100 "expression form outside the decided surface" | C24 row 18 |
| `debugger;` | S100 "statement form outside the decided surface" | C24 row 29 |

The coercion reason does not hold: `tsc` rejects `isNaN("a")` with
TS2345, so the argument of an accepted call is a number. For a number,
the global form and the `Number` form give the same result. `join`
already formats an array by the Q14 rules. The other two forms have no
run-time meaning.

### 204.1 Rules

1. `isNaN(x)` is `Number.isNaN(x)`, and `isFinite(x)` is
   `Number.isFinite(x)`. The argument rules and the texts of the
   `Number` forms apply.
2. `` `${xs}` `` with an array `xs` is `` `${xs.join(",")}` ``. `xs`
   evaluates once. If `join` rejects the element type of `xs`, the
   interpolation fails with the `join` text. A `FixedArray` is not in
   this rule.
3. `x satisfies T` checks `x` with the context `T`. The type of `x`
   must convert to `T` as in the assignment of `x` to a `T` local, or
   the expression fails with the text of that assignment. If `x`
   contains an empty array literal at any depth, `x` is checked with
   no context: `tsc` types `[]` as `never[]` there, so the empty
   literal gives its no-context diagnostic. *(Amended 2026-10-28 after
   the Phase Review, M1.)* The
   expression is `x`, and its type is the type of `x`. It adds no
   run-time operation.
4. `debugger;` is an empty statement. It adds no run-time operation.
4a. A value read of the global `isNaN` or `isFinite` fails with the
   text of the `Number` form: "`isNaN` may only be called, not read as
   a value".
5. These stay rejected with their existing texts: `<T>x`, `string +`
   an array, `String(xs)`, and a template operand of a class type.

### 204.2 Acceptance

1. Red first, at the pin `c258a296`: the accept entry `a390-trivial-erasures`
   is rejected. The reject entries `r449-satisfies-mismatch` and
   `r450-template-nested-array` fail their expected errors at the pin.
   The tracking note records each result.
2. `a390` passes `tsc`, and its output under `node` equals its golden.
   It interpolates an array of each element kind that `join` accepts
   (a string-literal alias element is not one of them, see §204.3),
   an empty array, and an array that a call gives (evaluated once).
3. `r449` gives the assignment mismatch text. `tsc` rejects it with
   TS1360. `r450` gives the `join` text.
4. A unit test shows that `` `${xs}` `` and `` `${xs.join(",")}` `` give
   the same HIR, with a control on a different separator. A unit test
   shows that `x satisfies T` and `x` give the same HIR.
5. The rejection sites and divergence entries that only these forms
   reached are removed. The rejection-site total check passes.
6. `stdlib.md` §11.1 and `collisions.md` C24 rows 18, 22, and 29 cite
   this section.
7. No golden moves, except the `a390` golden, which loses the
   `Mode[]` field (§204.3).

### 204.3 Phase Review

*(Added 2026-10-28.)* A fresh review of `c258a296..d433a4a7` gave no
CRITICAL or MAJOR finding and six MINOR findings.

1. MINOR, realistic, fixed: `[] satisfies string[]` gave the literal
   the type `string[]`, so a later `push("a")` was accepted. `tsc`
   types it `never[]` and gives TS2345. Rule 3 now checks such an
   operand with no context.
2. MINOR, contrived, open: `(this satisfies C).x = 1` in a
   constructor counts as the field assignment. `tsc` gives TS2564.
3. MINOR, fixed: `collisions.md` Q25 and the rule-boundary list, and
   `stdlib.md` §11.6 and §11.7, still named the global `isNaN` as a
   coercing form.
4. MINOR, fixed: C24 row 22 did not name the `FixedArray` operand.
5. MINOR, fixed: acceptance 7 did not name the `a390` golden change.
6. MINOR, open: a diagnostic names the form that rule 1 or rule 3
   gives: `isNaN()` names `Number.isNaN`, and a `satisfies` mismatch
   says "the initializer expects". `tsc` gives TS1360 "does not
   satisfy the expected type".

Open:

1. `join` rejects an array of a string-literal alias (`Mode[]`), so
   `` `${ms}` `` fails with the `join` text. `node` prints `on,off`. A
   literal-alias element is an `i32` index at run time, so `join`
   needs a new format kind in the runtime formatter.
