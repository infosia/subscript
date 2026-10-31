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
   narrows and flows as a class type does: `=== null`, `==`, narrowing
   by `if (x !== null)`, `??`, `??=`, and `?.` with a member or a call.
1a. A nullable array is a contextual type for an array literal, as its
   array type is. So `const c: f64[] | null = [1, 2];` and
   `b([])` with `b(x: f64[] | null)` are accepted. This holds in
   each position of a context, and for a nested element type. *(Added
   2026-10-31 after the Phase Review, M1.)*
2. A `T[] | null` whose array is a counted owner fails with the S011
   text of the pin, with a "TypeScript accepts" block. An array is a
   counted owner if its element is a counted handle or a counted owner:
   `Promise<T>[]` and `Promise<T>[][]` are. A class and a `Map` own
   their values, so `Map<K, Promise<T>>[]` and an array of a class with
   a `Promise` field are not, and `T[] | null` is legal for them. A
   nullable has no holder count, so the handles in a counted array have
   no owner: a fresh array stops with an internal error, and an alias
   reads a deleted handle.
2a. `Map.get` on a `Map` whose value is a counted owner fails with S014
   and names `getOr`. At `14a143f1` it gave `Promise<i32>[] | undefined`,
   the alias of rule 2: a later `set` or `delete` released the handles,
   and the three tiers trapped. `tsc` accepts it, and `node` prints the
   value. *(Added 2026-10-31 after the Phase Review, M2.)*
3. A mirror declaration, a foreign parameter, a foreign result, a mirror
   field, and a callback signature cannot have `T[] | null`. The form
   fails with S100 and names the declaration. The C ABI gives this form
   no layout.
4. `JSON.stringify` and `JSON.parse<D>` accept a field of type
   `T[] | null` and give the `node` result. A missing key raises, as
   for `C | null` (`stdlib.md`), where `node` leaves the field absent. A
   Worker message field stays rejected with its existing text.
5. `.length`, an index, a method call, and `for…of` on a nullable array
   stay rejected, as `tsc` TS18047 does. `x[i]` through `?.[i]` and
   `x!` keep the text that a nullable class has.
6. `a === b` with two arrays stays rejected. `a === null` is accepted.
7. §218 states `string | null`.
8. The form has no run-time cost beyond the null test that the source
   writes.
9. Each `FixedArray` type has its own C type. Two `FixedArray` types of
   one length with different handle element types compile on the ship
   tier. At `14a143f1`, `FixedArray<A, 2>` beside `FixedArray<B, 2>` gave
   a C error, "typedef redefinition". *(Added 2026-10-31 after the Phase
   Review, M3.)*

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
3a. `a400` pins rule 1a. `r460-map-get-counted-array` pins rule 2a: it
   fails at the pin as a run-time trap and passes as an S014 rejection.
   `a401-fixed-array-element-types` pins rule 9: at `7d2729b2` it fails
   on the ship tier only. A program with two class element types fails
   the same way at `14a143f1`.
4. The rejection-site total check passes.
5. C7 in `collisions.md` names an array. The language reference states
   the rule.
6. No golden moves. The LIR text snapshot does not move if `a400` has no
   generator and no async function.

### 217.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `14a143f1..7d2729b2` gave three
MAJOR and five MINOR findings.

1. MAJOR, realistic, fixed: an array literal took no context from
   `T[] | null`. `const c: f64[] | null = [1, 2];` gave S100, and `tsc`
   accepts it. Rule 1a.
2. MAJOR, realistic, fixed: `Map.get` gave a `Promise<i32>[] | undefined`
   local at `14a143f1`, and a later `set` released its handles. Rule 2a.
3. MAJOR, realistic, fixed: two `FixedArray` types of one length with
   handle elements failed the C compile at `14a143f1`. Rule 9.
4. MINOR, realistic, fixed: rule 2 did not state the test that the code
   applies. §196 Open item 4 is the same rule: a `Map` owns its values.
5. MINOR, realistic, fixed: rule 4 did not name a missing key.
6. MINOR, realistic, fixed: `operation_signatures` closed one site. The
   comparison now reads assignment to the table parameter.
7. MINOR, contrived, fixed: rule 1 said "`if`", and `if (x)` stays
   rejected.
8. MINOR, contrived, fixed: §123 said that a declaration cannot name
   `T[] | null`, and `?.[i]` and `x!` had no test.
