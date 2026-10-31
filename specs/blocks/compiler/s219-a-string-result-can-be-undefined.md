<!-- §219 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 219. A string result can be `undefined`, and equality narrows

*(Added 2026-10-31.)* Origin: the owner selected this section on
2026-10-31 with §218. The measurement is in
`specs/tracking/string-undefined-result.md`.

Problem, measured at `bbd38549`:

| Form | subscript | `tsc` 5.9.2 | `node` |
|---|---|---|---|
| `Map<K, string>.get(k)` | S014 "The value type has no `\| null` form" | accepts | `string` or `undefined` |
| `xs.find(f)`, `xs: string[]` | S014 "A scalar element type has no miss value" | accepts | `string` or `undefined` |
| `xs.pop()`, `xs: string[]`, empty | trap | accepts | `undefined` |
| `xs.pop().length` | accepted | TS18048 | throws on empty |
| `if (q === "lit") { q.length; }`, `q: string \| null` | S011 | accepts | runs |

§218 gives a string a null form, so the S014 reasons do not hold, and
§196.1 rule 2 lists `string` only for that reason.

### 219.1 Rules

1. `string` is in the set of `Map.get` (§123.1 rule 1, §195.5 rule 3). So
   `Map<K, string>.get`, `find` and `findLast` on `string[]`, and `pop`
   and `shift` on `string[]` give `string | undefined`. On a miss or an
   empty array they give `undefined`, and they do not trap.
2. §196.1 rule 2 drops `string` from its list. An empty `string[]` no
   longer traps on `pop` or `shift`.
3. `xs.pop().length` on `string[]` fails, as `tsc` TS18048 does. A
   `string` local cannot take the result with no test. This is the
   rule that §196 gives to a reference element.
4. **Equality narrows.** After `a === b` is `true`, or `a !== b` is
   `false`, where `b` has a type with no absence, `a` narrows to its
   non-null type. This holds for `==` and `!=`, for each operand order,
   and for a `string | null` and a `C | null`. The narrowing follows the
   rules of §159 and §162, so it ends at a call for a shared location.
   A test against a value that can be null gives no narrowing.
5. The S014 texts for `Map.get` and `find` no longer name `string` by any
   rule. The reference states the new set.
6. No run-time cost beyond the form that a `C | undefined` result has.

### 219.2 Acceptance

1. Red first, at the pin `bbd38549`: the accept entry
   `a404-string-undefined-result` is rejected. The tracking note records
   the result.
2. `a404` passes `tsc`, and its output under `node` equals its golden. It
   has `get`, `find`, `findLast`, `pop`, and `shift` on a hit and on a
   miss, `??` on each result, and rule 4 for a string and a class, in
   each operand order.
3. `r463-string-pop-needs-a-test` pins rule 3. `tsc` gives TS18048.
4. A trap entry or a golden that reads an empty `string[]` `pop` moves
   only if rule 1 names it. The tracking note lists each.
5. A unit test pins rule 4 with a control: a test against a nullable
   operand gives no narrowing.
6. The rejection-site total check passes. §196 and §123 cite this
   section.
