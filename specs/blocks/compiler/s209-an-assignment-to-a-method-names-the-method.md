<!-- §209 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 209. An assignment to a method names the method

*(Added 2026-10-29.)* Origin: the §208 Phase Review recorded a wrong
text for `t.v = t.v` through a constraint. On 2026-10-29 the owner
asked for the fix. The measurement is in
`specs/tracking/method-assignment.md`.

Problem, measured at the pin `8d4e0059`: `c.v = two;`, where `v` is an
instance method of `C`, gives S004 "nominal types are closed: `C` has
no property `v`". `C` has the member `v`, so the text is false. The
same text names `I` for `t.v = …` with `t: T` and `T extends I`.
`C.s = two;` for a static method gives S100 "static method `C.s` may
only be called". `tsc` 5.9.2 accepts each form: a method is a writable
property in TypeScript.

### 209.1 Rules

1. An assignment, a compound assignment, a logical assignment, an
   increment, or a decrement whose target is an instance method `m`
   fails with S100 "method `m` of `C` cannot be assigned: a method is a
   function, not a field". `C` is the type of the receiver, with its
   type arguments for a generic-class instance, or the interface `I`
   for a receiver of type `T extends I`.
2. The same forms on a static method `C.s` fail with S100 "static
   method `C.s` cannot be assigned: a method is a function, not a
   field".
3. A plain assignment and a logical assignment to a method that is not
   a `#x` method carry a "TypeScript accepts" block, because `tsc`
   accepts them (C24 row 12). The replacement is a field of a function
   type. A compound assignment, an increment, a decrement, and an
   assignment to a `#x` method carry no block: `tsc` rejects them
   (TS2365, TS2356, TS2803). *(Amended 2026-10-29 after the Phase
   Review, m1.)*
4. The rejection does not change: only the text and the site change.

### 209.2 Acceptance

1. Red first, at the pin `8d4e0059`: `r458-method-assignment` gives the
   S004 text. The tracking note records the result.
2. `r458` gives the rule 1 text. `tsc` accepts it.
3. A unit test pins rule 1 for a class receiver and a constrained
   receiver, and rule 2, each with the assignment and a compound
   assignment. A control assigns a field of a function type, which is
   accepted.
4. The rejection-site total check passes.
5. No golden moves.

### 209.3 Phase Review

*(Added 2026-10-29.)* A fresh review of `8d4e0059..cf813d89` gave one
MAJOR and two MINOR findings.

1. MAJOR, realistic, fixed: the divergence cited C24 row 12, which
   named only a method read as a value. Row 12 now names an assignment
   to a method.
2. MINOR, contrived, fixed: a compound assignment, an increment, a
   decrement, and an assignment to a `#x` method carried a "TypeScript
   accepts" block, but `tsc` rejects them. Rule 3.
3. MINOR, contrived, fixed: rule 1 said "the class of the receiver";
   the text names the receiver type with its type arguments.
