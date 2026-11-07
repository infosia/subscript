<!-- §235 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 235. A function type takes a nullable handle variance

*(Added 2026-11-07.)* Origin: the owner question of 2026-11-07 on
`collisions.md` C24 row 25. Under the owner's standing approval of
2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `95d6afb6` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `class C` is a reference class.

| Form | subscript | `tsc` |
|---|---|---|
| `(x: C): C => x` as an argument of the type `(x: C) => C \| null` | S100 | accepts |
| `(x: C \| null): i32 => 1` as an argument of the type `(x: C) => i32` | S100 | accepts |
| `const g: (x: C) => C \| null = (x: C): C => x` | S100 | accepts |
| `[c].reduce((acc: C \| null, x: C): C => x, null)` | S100 | accepts |
| `(): string => "a"` as `() => string \| null` | S100 | accepts |
| `(): i32[] => [1]` as `() => i32[] \| null` | S100 | accepts |
| `(): Map<i32, i32> => …` as `() => Map<i32, i32> \| null` | S100 | accepts |
| `(): () => i32 => …` as `() => (() => i32) \| null` | S100 | accepts |
| `(g: (x: C) => C) => {}` as `(g: (x: C) => C \| null) => void` | S100 | TS2345 |
| `const ys: (C \| null)[] = xs`, `xs: C[]` | S100 | accepts |

The divergence `FunctionParameterIdentity` states its reason: "the
language has no implicit conversion, structural substitution, or
variance". A nullable type is `Ref | null` only (S011). For a reference
class, `string`, an array, `Map`, and `Set`, the value of `T` and the
value of `T | null` have the same representation: one handle word
(`codegen/src/layout.rs` `repr`). A function value of the result type
`T` already gives a valid `T | null` result, with no conversion. The
reason does not hold for these result and parameter pairs.

A function type and a nullable function type have the same
representation, `Repr::Pair`, with `(SubFn){0}` as `null`. The reason
does not hold for a function result either.

The reason holds for the other rows. An array of `C` and an array of
`C | null` are one mutable location for two element types (C17).

### 235.1 Rules

1. A handle type is a reference class, `string`, an array, `Map`,
   `Set`, or a function type.
2. The relation "`S` fits `T`" holds when one of these is true:
   - `S` and `T` are the same type.
   - `T` is `S | null`, and `S` is a handle type.
   - `S` and `T` are function types, and rule 3 holds for them.
   - `S` is `S0 | null`, `T` is `T0 | null`, and `S0` fits `T0`.
3. A function type `F` is assignable to a function type `G` when they
   have the same number of parameters, and:
   - the result of `F` fits the result of `G`;
   - each parameter of `G` fits the parameter of `F` at its position.
4. Rule 3 applies at each place that compares types for assignment: an
   argument, an initializer, an assignment, a return value, a field
   value, and a built-in callback such as the `reduce` callback.
5. No conversion runs. The function value keeps its code and its
   environment. A call through the type `G` gives the same result
   bits as a call through `F`.
5a. At each place where rule 3 accepts two function types that are not
   identical, the checker records a conversion of the value to the
   type of the place. Each later stage reads the type of the place.
   This includes a built-in callback, a built-in value argument, such
   as an `Array.push` or a `Map.set` argument, the value of `??=`, and
   the right operand of `??`.
5b. The checker records the conversion only after the assignment check
   accepts the value. The target of the conversion is the type of the
   place, never a type derived from the value. A value that is not
   assignable keeps its diagnostic.
5d. Each check of a value against a place uses the check that takes the
   value, so that it can record the conversion. A check that takes
   only two types is used only where no value flows. A total check
   lists each remaining call of the type-only check with its reason.
5c. A converted built-in callback has the cost of the identical type: it
   keeps the inline lowering of a lambda or a named function. The cost
   does not depend on the declaration of the callback.
6. Each other difference of two function types stays rejected with its
   present diagnostic and site: an integer width
   (`FunctionParameterIdentity`), a value class, a type argument, and
   an array element. A form that `tsc`
   rejects stays rejected.
7. No accepted program changes meaning. Each accepted program before
   this section has identical function types at each such place.
8. The reason of `FunctionParameterIdentity` and the text of
   `collisions.md` C24 row 25 state the exception of rule 3 and cite
   this section.

### 235.2 Acceptance

1. Red first, at the pin: the accept entry
   `a422-function-type-nullable-handle-variance` is rejected. The
   tracking note records the output.
2. `a422` passes `tsc`. Its output under `node` equals its golden. It
   has the first eight rows of the table, a function value in a field
   and in a return value, and a call through each converted value. It
   also has a built-in callback with a `C | null` parameter for `C`
   elements (`map`, `filter`, `findIndex` with a named function, the
   element of `reduce`, `Map.forEach`), and a function value as an
   argument of `push` and of `Map.set`.
3. The reject entry `r491-function-type-variance-wrong-direction` has
   the row that `tsc` rejects. Its header states `tsc: rejects`.
4. A unit test pins rules 2, 3, and 6, with controls: an integer width,
   `C[]` to `(C | null)[]`, the wrong direction of a parameter, and the
   identical type. A test pins rule 5a: the HIR of each place of
   acceptance item 2 has the type of the place. Tests pin rule 5b: a
   value of the type `((x: C) => C) | null` is rejected at a
   `(x: C) => C | null` place, as `tsc` rejects it (TS2322, TS2345),
   and `v ??= f` with a nullable function value of a fitting type
   assigns the value. A test pins rule 5c: the LIR of a converted
   `forEach` callback has no indirect call, and no added root slot when
   the representations are the same. `a422` has a `??` with a function
   fallback of a fitting type.
5. The rejection-site total check passes. Each site keeps a witness of
   a form that it still rejects. If the omission count of
   `generic_tsc_matrix` changes, the tracking note records the old and
   the new count.
6. If a corpus entry or golden changes its type or output, report it
   and stop. `a422` has no async or generator function, so no golden
   other than its own moves.

### 235.3 Phase Review

*(Added 2026-11-07.)* Three fresh reviews of the implementation. Each
fix changed the form; the contract gained rules 5a–5d.

1. MAJOR, realistic, fixed: the checker accepted a non-identical
   function type with no conversion in HIR. A built-in callback with a
   `C | null` parameter over `C` elements, and a function value as a
   `push` or `Map.set` argument, stopped in LIR construction. Rule 5a:
   the checker records a conversion to the type of the place.
2. MAJOR, realistic, fixed: the same class at built-in value arguments
   (`unshift`, `fill`, `Map.getOr`).
3. MINOR, fixed: the contract stated that a nullable function type has
   another representation. Both are `Repr::Pair`. Rule 1 now names the
   function type.
4. CRITICAL, realistic, fixed: the conversion took its target from the
   value, so `((x: C) => C) | null` was accepted at a `(x: C) => C | null`
   place, and the program stopped with a fault. Rule 5b: the conversion
   follows the assignment check, and its target is the type of the
   place. Reject entry `r492` guards it.
5. MAJOR, realistic, fixed: `v ??= f` with a nullable function value of
   a fitting type assigned nothing, and `||=` gave no diagnostic. Rule 2
   relates two nullable function types.
6. MINOR, fixed: a converted built-in callback left the inline lowering
   (2.6 times the time on the development tier) and then added a root
   slot per call. Rule 5c: it keeps the inline lowering, with no added
   coercion.
7. MINOR, fixed: the codegen pass-through of a function value accepted
   any two function types. `Type::same_function_representation` limits
   it to the pairs of rule 2.
8. MAJOR, realistic, fixed: `n ?? id` with a fitting function fallback
   was accepted with the type `Error`, and both tiers stopped with an
   internal error. Rule 5d: each value check takes the value, and
   `require_assignable` rejects a pair that needs a conversion. The test
   `type_only_assignability` lists each remaining type-only call with
   its reason.
9. MINOR, fixed: a capture diagnostic printed a class id; a test sat
   above its `use`.

### 235.4 Open

1. A conditional with no context, `b ? (x: C): C => x : (x: C): C | null
   => null`, has no common type (S100). `tsc` accepts it.
2. Collection element types stay invariant: `Array.from<F>(ids)`,
   a spread, and a `Set` source with a fitting function element are
   rejected. `tsc` accepts them.
3. A `then` callback with a `C | null` parameter, generic inference of
   `T` from a fitting callback, and an interface method implemented with
   a fitting type stay rejected. `tsc` accepts them.
