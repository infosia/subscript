<!-- §218 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 218. A nullable string

*(Added 2026-10-31.)* Origin: the owner selected this form on 2026-10-31
after the Step 0 of §217. The measurement is in
`specs/tracking/nullable-string.md` and
`specs/tracking/nullable-array.md`.

Problem, measured at `ca7d8897`: `function f(x: string | null): void {}`
gives S011 "unions are limited to `Ref | null`; `string | null` is not a
reference type union". `tsc` 5.9.2 accepts it. `T[] | null` and
`Map<K, V> | null` are accepted, and `string | null` is the most common
nullable form in TypeScript.

A change of the `HandleKind` table alone gives wrong values on the
three tiers, which the differential gate cannot see (core principle 12):

| Form | `node` | Table change only |
|---|---|---|
| `(string \| null)[]`, `includes(mk())` | `true` | `false` |
| `indexOf(mk())` | `2` | `-1` |
| `s === t`, if the checker opens it | by content | by pointer |

The cause is one class: dispatch reads `Type::Str`, and `Nullable(Str)`
falls to the pointer form.

### 218.1 Rules

1. `string | null` is a legal `Ref | null` form (C7), in each position
   where `T[] | null` is legal (§217.1 rule 1). Its absence is a null
   pointer. An empty string is not absent: an `s: string | null` that
   holds `""` gives `false` for `s === null`.
1a. `string | undefined` is legal as a declared result, as §195 states
   for `T | undefined`. It compares by rule 2 and with `== null`. `===`
   against a `string | null` stays rejected (§195). *(Added 2026-10-31
   after the Phase Review, m4.)*
2. **One equality rule.** `===`, `!==`, `==`, and `!=` compare by
   content if one operand has type `string | null` and the other has
   type `string` or `string | null`. A null value equals a null value
   only. A test against the literal `null` is a null test. The rule has
   one definition, `Type::compares_string_content`.
   The LIR gives a nullable operand as `StringOperand`, and each tier
   reaches its one string compare. The verifier rejects `Binary(Eq|Ne)`
   on a nullable string, except a null test. The order of the operands
   does not change the result. A `string` operand against `null` stays
   rejected: `NonNullableNullEquality` applies, as for a class.
   *(Amended 2026-10-31: the coding agent measured the form.)*
3. **One element-kind table.** `includes`, `indexOf`, and `lastIndexOf` on
   `(string | null)[]` compare by rule 2. The element kind of an array has
   one definition, which the checker and each tier read.
4. **Total.** Each site that reads `Type::Str` for its behaviour either
   reads the rule for a nullable string or is shown not to take one. The
   tracking note lists the sites.
5. These forms stay rejected, and each text states the fix: a template
   operand, `+`, `+=`, `print`, and a `switch` discriminant of type
   `string | null`. `tsc` accepts the first three and the last, and
   `node` gives the string `null` in the first three. A null test first
   gives a `string`.
6. `.length`, an index, and a method on a `string | null` stay
   rejected, as `tsc` TS18047 does.
7. `join`, `toString()`, and `sort()` on `(string | null)[]`, and a
   `string | null` key of `Map` or `Set`, stay rejected.
8. `JSON.stringify` and `JSON.parse<D>` accept a `string | null` field and
   give the `node` result. A missing key raises, as for `C | null`.
9. A Worker message field, a host entry, a foreign parameter, a foreign
   result, a mirror field, and a callback signature cannot have
   `string | null`. §217.1 rule 3 applies. The C ABI of a `string` is a
   view, which has no null form.
10. A string-literal alias `L | null` stays rejected.
11. The form has no run-time cost beyond the null test in the string
    compare, which the runtime compare does.
12. No value other than an `Address` holds an address. A `FixedArray`
    cursor holds an index, and `IteratorValue` takes the array as an
    operand, so the array has a use at each read. Liveness, a suspension,
    and a slot clear follow from that use. A `FixedArray` call result that
    a direct index reads has a base slot in the same way. At `ca7d8897`,
    `for (const x of b)` with `b: FixedArray<string, 2>` read null
    elements on the JIT and the ship tier, a body with `await` or `yield`
    stopped on a bad read, and `pair()[0]` printed nothing, with no
    diagnostic. *(Added 2026-10-31 after the Phase Review, M1.)*
13. `reduce` and `reduceRight` convert the seed to the accumulator type,
    as an argument is converted. So a `string` or `null` seed for a
    `string | null` accumulator is accepted, and for `C | null` and
    `T[] | null`. At `ca7d8897` this form gave an internal error.
    *(Added 2026-10-31 after the Phase Review, M2.)*

### 218.2 Acceptance

1. Red first, at the pin `ca7d8897`: the accept entry
   `a402-nullable-string` is rejected. The tracking note records the
   result. `r461-nullable-string-template` passes at the pin with the S011
   text, and with the rule 5 text after the fix.
2. `a402` passes `tsc`, and its output under `node` equals its golden.
   It has each position of rule 1, a `string` built at run time and
   compared by content in both operand orders, `""` against `null`,
   `includes` and `indexOf` on `(string | null)[]`, `??`, `?.`, a
   `Context.collect()` with a null field, and `JSON.stringify`.
3. A unit test shows that each tier reads one definition of rule 2 and
   rule 3.
3a. `a403-for-of-fixed-array-handles` pins rule 12, with `string`,
   `string | null`, and `C | null` elements and two loops. It fails at
   `ca7d8897` on the JIT and the ship tier for the `string` form. `a402`
   pins rule 13 and rule 1a.
4. The rejection-site total check passes.
5. C7 in `collisions.md` names a string. §217.1 rule 7 cites this section.
6. No golden moves. The LIR text snapshot does not move if `a402` has no
   generator and no async function.

### 218.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `ca7d8897..821d2a7a` gave two
MAJOR and seven MINOR findings. §218 gave no wrong value in the review.

1. MAJOR, realistic, fixed: `for…of` over a `FixedArray` of handles read
   null elements on the JIT and the ship tier at `ca7d8897`. The
   iterator held the address of a root temporary, and the next line
   cleared it. Rule 12.
2. MAJOR, realistic, fixed: `reduce` with a nullable accumulator and a
   plain seed gave an internal error at `ca7d8897`. Rule 13.
3. MINOR, realistic, fixed: the reference and rule 2 named `"" === null`
   and `null` as a content operand.
4. MINOR, realistic, fixed: a doc comment moved to another function.
5. MINOR, realistic, closed by §219: the S014 texts for `Map.get` and
   `find` on `string` were false.
6. MINOR, realistic, fixed: `string | undefined` was accepted and no rule
   stated it. Rule 1a.
7. MINOR, realistic, closed by §219: `s === "lit"` gave no narrowing.
8. MAJOR, realistic, fixed: the review named a second `getOr` on a `Map`
   with a `FixedArray` value as MINOR. The cause is wider: `pair()[0]`
   on any `FixedArray` call result printed nothing on the JIT and the
   ship tier. Rule 12.
9. MINOR, contrived, fixed: the verifier scanned the function for each
   `StringOperand`.

### 218.4 Open

1. `let b: FixedArray<i32, 2>`, `for (const x of b) { b[1] = 9; print(x); }`
   prints `1 2` on the three tiers. `node` prints `1 9`. This predates
   §218. `collisions.md` names no such divergence.

