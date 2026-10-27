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
   a value class and of a boundary struct, and the key order of
   `JSON.stringify`, follow this order.
3. **Initialization order.** The field initializers run first, as
   they do at the pin (C9). Then the constructor assigns each
   parameter property from its parameter, in parameter order. Then
   the constructor body runs. This is the order of the `tsc` output.
4. A parameter-property field has no initializer. A field initializer
   that reads it is rejected by §147 rule 2, as `tsc` rejects it
   (TS2729).
5. A field and a parameter property with the same name are rejected
   (S017), as `tsc` rejects them (TS2300).
6. A parameter property needs a type annotation. Without one it is
   rejected (S100) with "a parameter property requires a type
   annotation". `tsc` accepts `constructor(public count = 0)` and
   takes the type of the default; the field type comes from the
   declaration before the checker infers the default.
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
