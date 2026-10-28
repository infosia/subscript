<!-- §205 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 205. A class cannot declare `implements`

*(Added 2026-10-28.)* Origin: on 2026-10-28 the owner asked why
`interface` is not in the language, and decided that a form that the
checker accepts with no meaning is rejected explicitly. The
measurement is in `specs/tracking/implements-clause.md`.

Problem, measured at the pin `40cf8887`: `class B implements A { … }`, where
`A` is a class, gives no diagnostic. The clause has no effect: a `B`
value does not convert to `A` (S005 "nominal types are not
interchangeable"), and the checker does not compare the members of `B`
with the members of `A`. `tsc` 5.9.2 checks the members, and it
converts a `B` value to `A` because its types are structural. So a
reader of the source sees a contract that the language does not keep.
`interface` is already rejected (C24 row 6), so the clause can name
only a class.

### 205.1 Rules

1. A class declaration with an `implements` clause fails with S100
   "a class cannot declare `implements`: types are nominal (C1), so
   the clause gives no conversion". The diagnostic is at the clause.
   This applies to a reference class, a value class, a generic class,
   and a clause with more than one name.
2. The diagnostic carries a "TypeScript accepts" block. The
   replacement is the same class with no clause.
3. `collisions.md` C24 row 6 names the `implements` clause.

### 205.2 Acceptance

1. Red first, at the pin `40cf8887`: the reject entry
   `r451-class-implements` gives no diagnostic at the pin. The
   tracking note records the result.
2. `r451` gives the rule 1 text. `tsc` accepts it.
3. The new rejection site has a witness, and the rejection-site total
   check passes.
4. A unit test pins rule 1 for a value class, a generic class, and a
   clause with two names, with a control: the same class with no
   clause is accepted.
5. No golden moves.
