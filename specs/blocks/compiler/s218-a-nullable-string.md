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
2. **One equality rule.** `===`, `!==`, `==`, and `!=` compare by
   content if one operand has type `string | null` and the other has
   type `string`, `string | null`, or `null`. `null` equals `null`
   only. The rule has one definition, `Type::compares_string_content`.
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
4. The rejection-site total check passes.
5. C7 in `collisions.md` names a string. §217.1 rule 7 cites this section.
6. No golden moves. The LIR text snapshot does not move if `a402` has no
   generator and no async function.
