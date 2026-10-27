<!-- §198 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 198. A constructor parameter property declares a field

*(Added 2026-10-27.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove, and selected
this one second. The measurement is in
`specs/tracking/constructor-parameter-properties.md`.

Problem, measured at the pin `27093316`: `constructor(public x: i32)`
gives S100 "constructor parameter properties are not decided", and
each read of `x` gives S018. `tsc` 5.9.2 accepts the form, and `node`
runs it. `collisions.md` C24 row 8 gives no reason ("No lowering is
decided").

### 198.1 Rules

1. A constructor parameter with a modifier `public`, `private`,
   `protected`, or `readonly`, or a combination that `tsc` accepts, is
   a parameter property. It declares an instance field with the name,
   the type, and the modifiers of the parameter. It is also a
   parameter of the constructor, with its default if it has one.
2. **Field order.** The parameter-property fields come first, in
   parameter order. The declared fields follow, in declaration order.
   This is the order of the `tsc` output (ES2022): it declares each
   parameter-property field before the class fields. The C layout of
   a value class and the key order of `JSON.stringify` follow this
   order. A mirror `.d.ts` has no constructor implementation, so a
   boundary struct has no parameter property (`tsc` gives TS2369).
3. **Initialization order.** The field initializers run first, as
   they do at the pin (C9). Then the constructor assigns each
   parameter property from its parameter, in parameter order. Then
   the constructor body runs. This is the order of the `tsc` output.
4. A parameter-property field has no initializer. A field initializer
   that reads it is rejected by §147 rule 2, as `tsc` rejects it
   (TS2729).
5. A field and a parameter property with the same name are rejected
   (S017), as `tsc` rejects them (TS2300).
5a. The field and the parameter share one annotation. The checker
   reports each diagnostic that both give once: a second diagnostic
   with the same code, message, and position is dropped. No other
   diagnostic is dropped, so each rule that checks a field type also
   checks a parameter-property field. *(Added 2026-10-27 after the
   Phase Review, M1.)*
6. A parameter property with no type annotation and a literal
   default takes the type that C4 gives the literal: an integer
   literal `i32`, a fractional literal `f64`, a string literal
   `string`, and `true` or `false` `boolean`. `tsc` accepts
   `constructor(public count = 0)`. With no annotation and any other
   default, the form is rejected (S100) with "a parameter property
   requires a type annotation". *(Amended 2026-10-27 after the Phase
   Review, m3: the first text rejected a literal default too, for a
   reason of the checker order only.)*
7. The form has no run-time cost. It lowers to the field and the
   assignment that a program writes by hand.

### 198.2 Acceptance

1. Red first, at the pin `27093316`: `a379-constructor-parameter-properties`
   and `a380-value-class-parameter-properties` are rejected with S100.
   `r442-parameter-property-read-by-field-initializer` gives the
   S100 "constructor parameter properties are not decided" first. The
   tracking note records each result.
2. `a379` passes `tsc`, and its output under `node` equals its golden.
   `a380` is not `js-comparable` (C2); its `bytesOf` length pins the
   field order of rule 2.
3. A unit test pins rule 3 with a field initializer, a parameter
   property, and a body that each record their order.
4. A unit test pins rule 5.
5. `collisions.md` C24 row 8 keeps only the string-literal enum member
   name.
6. No golden moves.

### 198.3 Phase Review

*(Added 2026-10-27.)* A fresh review of `27093316..bca8d03d` gave one
MAJOR and six MINOR findings.

1. MAJOR, realistic, fixed: the checker dropped every diagnostic of
   the field annotation to report a shared annotation once. The first
   resolution of a generic instance checks its shape once, so
   `constructor(public slot: Slot<Enemy>)` on a value class `Slot<T>`
   with a reference element was accepted and ran. The hand-written
   form gives S100 "field type `Enemy` is outside the value-class
   whitelist". Rule 5a states the fix.
2. MINOR, fixed: `constructor(private group: TaskGroup)` lost the
   field-position text (same cause).
3. MINOR, fixed: rule 6 rejected a literal default.
4. MINOR, fixed: rule 2 named a boundary struct.

Open:

1. A value-class parameter property that the body writes through
   (`pos.x = 9.0`) writes the parameter copy, as the hand-written form
   does (C2). W004 does not fire, because the rewrite reads `pos`.
2. `constructor(public x)` gives two S100 texts at one position. `tsc`
   rejects it (TS7006).
3. A `@Descriptor` class with a parameter property first reports the
   descriptor member text, then the constructor text.

