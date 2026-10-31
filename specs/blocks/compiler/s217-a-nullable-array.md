<!-- §217 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 217. A nullable array

*(Added 2026-10-31.)* Origin: on 2026-10-31 the owner asked for the
open work in priority order. §212 measured `fa(null)` with
`fa<T>(x: T[] | null)`. The measurement is in
`specs/tracking/nullable-array.md`.

Problem, measured at `14a143f1`: `function f(x: i32[] | null): void {}`
gives S011 "unions are limited to `Ref | null`; `i32[] | null` is not a
reference type union". `tsc` 5.9.2 accepts it. `Map<K, V> | null` and
`Set<T> | null` are accepted. An array is a heap handle, as a map is, and
no record gives a reason for the difference.

### 217.1 Rules

1. `T[] | null` is a legal `Ref | null` form (C7), in each position
   where `Map<K, V> | null` is legal. Its absence is a null pointer. It
   narrows and flows as a class type does: `=== null`, `==`, `if`
   narrowing, `??`, `??=`, and `?.` with a member or a call.
2. A `T[] | null` whose element type holds a counted value fails with
   the S011 text of the pin, with a "TypeScript accepts" block. An array
   of `Promise<T>` is one. A nullable has no holder count, so the handles
   in the array have no owner. *(C7 measurement: a fresh array stops
   with an internal error, and an alias reads a deleted handle.)*
3. A mirror declaration, a foreign parameter, a foreign result, a mirror
   field, and a callback signature cannot have `T[] | null`. The form
   fails with S100 and names the declaration. The C ABI gives this form
   no layout.
4. `JSON.stringify` and `JSON.parse<D>` accept a field of type
   `T[] | null` and give the `node` result. A Worker message field stays
   rejected with its existing text.
5. `.length`, an index, a method call, and `for…of` on a nullable array
   stay rejected, as `tsc` TS18047 does. `x[i]` through `?.[i]` and
   `x!` keep the text that a nullable class has.
6. `a === b` with two arrays stays rejected. `a === null` is accepted.
7. A `string | null` stays rejected. Its equality and search need a
   separate section.
8. The form has no run-time cost beyond the null test that the source
   writes.

### 217.2 Acceptance

1. Red first, at the pin `14a143f1`: the accept entry `a400-nullable-array`
   is rejected. The reject entry `r459-nullable-array-of-handles` passes
   with the S011 text at the pin and stays rejected. The tracking note
   records each result.
2. `a400` passes `tsc`, and its output under `node` equals its golden.
   It has each position of rule 1, a generic argument, a `??` fallback,
   a `Context.collect()` with a null field, and `JSON.stringify` of a
   null field.
3. A unit test pins rule 3 for a foreign parameter and a mirror field.
4. The rejection-site total check passes.
5. C7 in `collisions.md` names an array. The language reference states
   the rule.
6. No golden moves. The LIR text snapshot does not move if `a400` has no
   generator and no async function.
