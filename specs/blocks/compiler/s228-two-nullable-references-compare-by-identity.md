<!-- §228 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 228. Two nullable references compare by identity

*(Added 2026-11-06.)* Origin: §219.4 item 1. The owner selected this
section on 2026-11-06.

Problem, measured at `68929cab` with `tsc` 5.9.2 and the debug CLI. `C`
is a class, and `a` and `b` are parameters:

| Form | subscript | `tsc` |
|---|---|---|
| `a: C \| null`, `b: C \| null`: `a === b` | S100 "operator `===` not defined" | accepts |
| the same, `a != b` | S100 | accepts |
| `n: N` with `next: N \| null`: `a.next === b.next` | S100 | accepts |
| `m: Map<i32, C>`: `m.get(1) === m.get(2)` | S100 | accepts |
| `Map<i32, i32> \| null` against itself | S100 | accepts |
| `C \| null` against `C`, `Map \| null` against `Map` | accepts (§219.1 rule 4a) | accepts |
| `string \| null` against itself, `===` and `==` | accepts | accepts |
| `Map<i32, string>`: `m.get(1) === m.get(2)` | accepts | accepts |

A nullable string pair compares, and a nullable reference compares
against its non-null type. A nullable reference pair has no reason to
differ. A linked structure, such as `a.next === b.next`, reaches this
form.

### 228.1 Rules

1. If `T | null === T` is accepted, `T | null === T | null` is accepted.
   The same holds for `| undefined`, and for `!==`, `==`, and `!=`.
2. The comparison is by identity, as §219.1 rule 4a states. Two absent
   values of the same spelling are equal. An absent value and a present
   value are not equal.
3. `==` and `!=` accept `T | null` against `T | undefined`, and two
   absent values are equal. This is the rule that a string pair has.
4. `===` and `!==` of `T | null` against `T | undefined` stay rejected
   with S012 (§195, `r426`).
5. A pair of two different types stays rejected, as it is with no
   `| null`.
6. Two nullable operands give no narrowing (§219.1 rule 4).
7. No run-time cost beyond the identity comparison of rule 2.

### 228.2 Acceptance

1. Red first, at the pin: the accept entry
   `a417-nullable-references-compare` is rejected with S100. The tracking
   note records the output.
2. `a417` passes `tsc`. Its output under `node` equals its golden. It
   has each rejected row of the table, `!==`, `==`, and `!=`, rule 3, and
   for each form the cases both absent, one absent, the same object, and
   two objects.
3. A unit test pins rule 1 for each type that `T | null === T` accepts,
   and pins rules 4, 5, and 6 with a control each.
4. The rejection-site total check passes. §219.1 rule 4a and §219.4
   item 1 cite this section. No other golden moves.
