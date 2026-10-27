<!-- §199 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 199. Logical assignment is a tested assignment

*(Added 2026-10-27.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove, and selected
this one third. The measurement is in
`specs/tracking/logical-assignment.md`.

Problem, measured at the pin `c3903596`: `x ??= v`, `a ||= b`, and
`a &&= b` give S100 "assignment operator outside the decided
surface". `tsc` 5.9.2 accepts them, and `node` runs them. §82.3 rule 4
kept `??=` out because it had no HIR form. `collisions.md` C24 row 18
gives no reason for `&&=` and `||=`. The spelled forms are accepted
at the pin: `if (x === null) { x = v; }` and
`x !== null ? x : (x = v)` for a local and a field.

### 199.1 Rules

1. `p ??= v` is `p !== null ? p : (p = v)`. `p ||= v` is
   `p ? p : (p = v)`. `p &&= v` is `p ? (p = v) : p`. In each form,
   `v` evaluates only when the assignment runs, as in JavaScript.
2. `p` is a place that a plain assignment accepts: a local, a field,
   a static field, an array element, an accessor, or an index
   signature. The receiver and the index of `p` evaluate once, by
   §82.1 rule 3. An accessor and an index signature follow §82.1
   rules 1 and 4: the read runs once, and the write runs only when
   the assignment runs.
3. **Types.** For `??=`, `p` must be nullable (`T | null` or
   `T | undefined`), and `v` checks with the context `T`. If `v` has
   type `T`, the expression has type `T`. If `v` has the type of `p`,
   the expression has the type of `p`. Each other type of `v` fails
   as the plain assignment `p = v` fails. For `||=` and `&&=`, `p`
   and `v` must be `boolean` (C24 row 1), and the expression has type
   `boolean`.
4. **Narrowing.** After `p ??= v` in statement position, with `v` of
   type `T`, `p` narrows to `T`, as after
   `if (p === null) { p = v; }`. The narrowing ends where §124 ends
   it.
5. The expression is legal in statement position and in value
   position. In an initializer that has no statement list, a non-place
   receiver fails as §82.1 rule 3 states. An accessor or an index
   signature place is legal in statement position only, as §82.1
   rule 5 has it for a compound assignment: in value position it fails
   with the existing "cannot be used as a value" text.
6. A left operand that is not nullable fails for `??=` with S100
   "the left operand of `??=` has type `T`, which is not nullable".
   A left operand or a right operand that is not `boolean` fails for
   `||=` and `&&=` with S100 "`||=` requires a boolean operand, got
   `T`" (or `&&=`).
7. The form has no run-time cost beyond the spelled test.

### 199.2 Acceptance

1. Red first, at the pin `c3903596`: `a381-logical-assignment` is rejected
   with S100. `r444-logical-or-assignment-on-string` and
   `r445-nullish-assignment-on-non-nullable` give the S100 "assignment
   operator outside the decided surface". The tracking note records
   each result.
2. `a381` passes `tsc`, and its output under `node` equals its golden.
   It counts the evaluations of the right operand and of a non-place
   receiver.
3. A unit test shows that `x ??= v` and the rule 1 spelling give the
   same HIR for a local place, with a control on a different spelling.
4. A unit test pins rule 4 for a local and for a `this` field, and the
   end of the narrowing at a call (§124), with controls.
5. `collisions.md` C24 row 18 drops `&&=` and `||=`. §82.3 rule 4 cites
   this section.
6. No golden moves.
