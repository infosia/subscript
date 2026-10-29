<!-- §208 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 208. An interface is a constraint

*(Added 2026-10-29.)* Origin: on 2026-10-28 the owner asked how far
`interface` and `implements` can go without a change to the paradigm
of the language. On 2026-10-29 the owner selected stage 1: an
interface as a type-only constraint. The measurement is in
`specs/tracking/interface-constraint.md`.

Problem, measured at the pin `3e9bd6c7`: `interface I { … }` gives S100
"declaration form outside the decided surface" (C24 row 6), and
`class C implements I` gives the §205 text. So a program cannot state
that two classes share a set of methods, and a generic function cannot
call a method on a type parameter: `function f<T>(x: T) { x.area(); }`
gives S018 "`T` has no method `area`". `tsc` 5.9.2 accepts these
forms. A generic function already takes a constraint
(`T extends C`), and each instance is a separate function (§140,
§149), so a call through a constraint is a direct call.

### 208.1 Rules

1. **Declaration.** A module-level `interface I { … }` declares a
   type-only interface. A member is a method signature
   (`m(a: A): R;`) or a read accessor signature (`get g(): R;`).
   `export interface` exports it, and `import` and `import type`
   import it (§128). An interface has no layout, no C name, and no
   run-time form.
2. These interface forms fail with S100 "interface `I` has a member
   form outside the decided surface", with a "TypeScript accepts"
   block: a property signature, an optional member, a call, construct,
   or index signature, a generic method signature, a type parameter on
   the interface, `extends` on the interface, and a second declaration
   of one name (`tsc` merges it), a write accessor signature, a rest
   parameter, and an overload signature. An interface in a function body
   keeps the C24 row 5 text.
3. **`implements`.** `class C implements I, J` declares that `C` has
   each member of `I` and `J`. For each method signature, `C` has an
   instance method with that name, the same parameter types, and the
   same result type. For each read accessor signature, `C` has an
   instance read accessor or an instance field with that name and
   type. A `private` member and a static member do not count. A method
   signature with the result `Promise<R>` is implemented by an `async`
   method with the result `Promise<R>`, and a call through a
   constraint can `await` it. A missing member fails with S100 "class
   `C` does not implement `m` of interface `I`", as `tsc` TS2420 does.
   A member with other types fails with S100 "class `C` implements `m`
   of interface `I` with type `X`; the interface requires `Y`". `tsc`
   accepts many of these members (a different integer width, an extra
   default parameter, a sync member for an async one) and gives TS2416
   for others, so this diagnostic carries a "TypeScript accepts" block
   (C24 row 6). A class member that failed its own check gives no
   second diagnostic here. *(Amended 2026-10-29 after the Phase Review,
   M1 and M2.)* A value
   class and a generic class can declare `implements`. An
   `implements` clause that names something other than an interface
   fails with the §205 diagnostic, and its text changes to
   "`implements` names an interface; `A` is not an interface"
   (§205.1 rule 1).
4. **A constraint.** A type parameter `T extends I` of a function, a
   method, or a class can call each member of `I` on a value of type
   `T`. A type argument for `T` is a class that declares
   `implements I`, or a type parameter whose constraint is `I` or a
   class that declares `implements I`. Another type argument fails
   with S100 "`A` does not declare `implements I`", also if `A` has the
   members (C1). `tsc` accepts such a class, so the diagnostic carries
   a "TypeScript accepts" block. A type parameter with no such
   constraint gets no block: `tsc` rejects it with TS2345. The rule applies to an inferred type argument
   (§149, §197) and to a written one.
5. **No value type.** An interface in any other type position fails
   with S100 "interface `I` is a constraint only; write a type
   parameter `T extends I`": a parameter, a result, a local, a field,
   an array element, a union, and a type argument. `tsc` accepts these
   forms, so the diagnostic carries a "TypeScript accepts" block. A
   value of an interface type needs a dispatch on the run-time class,
   which is not in this section.
6. `T extends I & J` and a constraint that is a union stay rejected.
7. The forms have no run-time cost. Each instance of a generic
   function or class calls the member of its type argument directly.

### 208.2 Acceptance

1. Red first, at the pin `3e9bd6c7`: the accept entry
   `a394-interface-constraint` is rejected. The reject entries
   `r455-interface-value-type`, `r456-class-does-not-implement`, and
   `r457-constraint-needs-implements` fail their expected errors at
   the pin. The tracking note records each result.
2. `a394` passes `tsc`, and its output under `node` equals its golden.
   It declares an interface in one module and imports it in another.
   It uses a method and a read accessor member, a class with two
   interfaces, a generic function, a generic class, and an inferred
   and a written type argument.
3. `r456` passes `tsc` with TS2420. `r455` and `r457` pass `tsc`.
4. A unit test shows that a call `x.m()` with `x: T`, `T extends I`,
   gives the same HIR in the instance for `C` as `c.m()` with `c: C`.
   A unit test pins rule 3 for a parameter type, a result type, a
   private member, and a static member, each with a control.
5. The rejection site of `interface` and the divergence entry change
   to the rule 2 and rule 5 sites. The rejection-site total check
   passes.
6. `collisions.md` C24 row 6 states the stage 1 form. §205 cites this
   section: its rejection now covers an `implements` clause that names
   a class.
7. No golden moves. The LIR text snapshot grows by the `a395` section
   only, because `a395` has async functions.

### 208.3 Phase Review

*(Added 2026-10-29.)* A fresh review of `3e9bd6c7..48aae770` gave two
MAJOR and five MINOR findings.

1. MAJOR, realistic, fixed: a member with a `Promise` result had no
   implementation. An `async` method did not count, and a sync method
   could not be awaited through a constraint. Rule 3 now accepts an
   `async` method. `a395` pins it.
2. MAJOR, realistic, fixed: a member with other types gave no
   "TypeScript accepts" block, but `tsc` accepts many such members. Rule
   3 now has two diagnostics, and C24 row 6 names the divergence.
3. MINOR, realistic, fixed: a class member that failed its own check
   also gave "does not implement".
4. MINOR, realistic, fixed: an unconstrained type parameter as a type
   argument got a "TypeScript accepts" block; `tsc` gives TS2345.
5. MINOR, fixed: rule 2 did not list a write accessor, a rest
   parameter, and an overload, and rule 4 did not list a constrained
   type parameter as a type argument.
6. MINOR, contrived, fixed by §209: `t.v = t.v`, where `v` is a method
   of `I`, gave "`I` has no property `v`". A class receiver gave the
   same text.
7. MINOR, realistic, fixed: the "does not implement" text did not give
   the types. The text for a member with other types now gives both.
