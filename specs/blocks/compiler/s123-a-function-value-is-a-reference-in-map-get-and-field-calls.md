<!-- §123 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 123. A function value is a reference in `Map.get` and in a field call

*(Added 2026-09-28.)* Origin: the review of §122
(`specs/tracking/s122-nullable-function.md`, open items). The owner
chose to fix both on 2026-09-28.

Problem, measured through `subscript run` at `8bc6ded`; `tsc` accepts
every probe:

- `Map<string, (x: i32) => i32>.get("d")` is rejected: `S014: get(key)
  is rejected: A scalar value type has no null miss value; use getOr
  (Q24)`. `Map<string, C>.get` with a reference class `C` is accepted.
  stdlib §10.5 lists the "nullable-capable" value types as a reference
  class and a handle. C7 counts a function type as a `Ref` of
  `Ref | null`, and §122 makes that form work, so the list is short by
  one kind. The same message is given for `Generator<V>`, which is not
  a scalar either (§106.4 item 4).
- A call through a function-typed field is rejected: `h.cb(4)`,
  `this.cb(5)`, and `h.cb(4)` inside `if (h.cb !== null)` all give
  `S018: H has no method cb`. The message names the wrong cause, and
  the program has to copy the field into a local first. A null check
  on a field path narrows a reference-class field (`if (h.c !== null)
  { h.c.v }` runs).

### 123.1 Rules

1. `Map<K, V>.get` returns `V | null` where `V` and `V | null` share one
   nullable-pointer representation: a reference class, a reference
   container (`Map`, `Set`), a function type, an array, and a
   nullable form of one of them. `Worker`, `Inbox`, and `Outbox` are
   not container type arguments (§40), so `get` has no rule for them
   here. One predicate decides the set, from
   the handle kind of `V` itself. A type whose `| null` form is a
   different representation is outside the set: a boundary struct
   (by-value storage; its `| null` is a box), and a handle for which
   S011 rejects `| null` (`RegExp`, `Date`, `Generator<T>`, an async
   handle). The array case is an existing decision (`a66`); its result
   `T[] | null` is usable in a null test though a declaration cannot
   name it. `get` on any type outside the set is rejected (S014).
2. The S014 reason names the kind of `V` it rejects. A numeric, a
   boolean, an enum, or a string-literal alias gets "a scalar value
   type has no null miss value". Every other rejected type (`string`,
   `Date`, `RegExp`, `Generator<T>`, a value class, `FixedArray`, a
   boundary struct) gets the reason that it has no `| null` form of the
   map's value representation. `getOr` stays the total spelling for
   both.
3. `recv.name(args)` where `name` is a field of function type, not a
   method, calls the field value with `args`. This includes
   `this.name(args)`, `recv?.name(args)`, a static field
   (`C.name(args)`), and a function-typed field of a boundary struct.
   The call reads the same value that `const f = recv.name` reads. `recv` is evaluated once,
   before the arguments. The call is the call of a function value
   (§118: a function value is an indirect call).
4. A field of type `F | null`, instance or static, narrows on its path
   as a reference-class field does (rule 1 of §122 over field paths, §46). `recv.name(args)`
   on an unnarrowed `F | null` field is rejected with the diagnostic a
   call of an unnarrowed `F | null` local gets.
5. A class declares a member name once (a field or a method), as `tsc`
   requires, so rule 3 cannot meet a method of the same name.
6. *(Corrected 2026-09-28.)* A first version of this rule rejected a
   call through a boundary-struct function field, on the claim that the
   field holds a C function pointer. The claim was not measured and is
   false: the emitted C stores a script function pair in the field
   (`(SubFn){ (void*)&sub_f2, NULL }`), and `const cb = info.callback;
   cb(x)` is accepted at `8bc6ded` and runs. Rule 3 covers the field
   call; the boundary conversion of §13 does not change.
7. `m.getOr(k, null)` on a `Map<K, V | null>` is a call with a null
   argument of a nullable reference type; it lowers like any other such
   argument (at `8bc6ded` it is an internal compiler error on both
   tiers).
8. Open item outside §123: §40 says an affine type is illegal as any
   container type argument, and `r111` pins a module-global
   `Map<i32, Worker<…>>`. A local `new Map<i32, Worker<M, M>>()` passes
   `subscript check` (measured on the §123 working tree). The text and
   the checker disagree; a later section decides which one is right.
9. Open item, decided in §124: a narrowed field path is not invalidated
   by a call or by a store through an alias, for a reference-class
   field and for a function field alike. Measured by the §123 review:
   `if (h.cb !== null) { clear(h); h.cb(3); }` and the same shape on
   `h.c.v` end in signal 11 on the dev JIT and status 139 on ship C.
   §123 does not change that rule.

### 123.2 Acceptance

1. Red first (core principle 10): a new accept entry
   `a272-function-value-reference-uses` fails at `8bc6ded` and passes
   after the change on all three engines, byte-identical to its
   `.expected`. It covers: `get` hit and miss on a `Map` of a function
   type; `getOr` on the same map; `h.cb(x)` and `this.cb(x)` on a
   function-typed field; a narrowed `F | null` field called through its
   path; a receiver expression with a side effect, to show it is
   evaluated once. `js-comparable` if `node` gives the same output;
   `tsc` accepts it.
2. A reject entry pins rule 4: a call on an unnarrowed `F | null` field.
   Its header states what `tsc` does, measured.
3. A test pins the S014 reason of rule 2 for each kind it names.
   A reject entry pins `get` on a boundary-struct value (S014), with a
   checker witness. An accept entry or a test pins a call through a
   boundary-struct function field, a narrowed static `F | null` field call, and `getOr(k, null)`
   on a nullable-valued map.
4. A `js-comparable` accept entry `a273-function-field-calls` pins rule
   3 without `get` or `getOr`, so `node` checks the field-call shapes.
5. No existing `.expected` golden moves. The LIR text snapshot moves
   only if a272 enters it.
