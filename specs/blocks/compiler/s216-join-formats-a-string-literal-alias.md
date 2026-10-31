<!-- §216 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 216. `join` formats a string-literal alias

*(Added 2026-10-31.)* Origin: §204.3 Open item 1. On 2026-10-31 the
owner asked for the open work in priority order. The measurement is in
`specs/tracking/join-string-alias.md`.

Problem, measured at `673b1113`: with `type Mode = "on" | "off"` and
`ms: Mode[]`, `ms.join(",")` gives S014 "`Mode` elements are not
interpolatable (Q22)", and `` `${ms}` `` gives the same text. `ms.map`,
`filter`, `forEach`, `includes`, and `indexOf` give S014 "`Mode` elements
are outside that set (Q22)". A single `` `${m}` `` and `for (const x of
ms)` are accepted. `tsc` 5.9.2 accepts each form, and `node` prints
`on,off`. An alias value is an `i32` discriminant, as an enum value is,
and an array of an enum takes these methods.

### 216.1 Rules

1. `xs.join(s)` with `xs: A[]`, where `A` is a string-literal alias,
   is `xs.map((x: A): string => `${x}`).join(s)`. `xs` evaluates once.
   `join()` with no argument uses `","`, as for another element type.
2. `` `${xs}` `` follows, because §204.1 rule 2 gives it the `join`
   form.
1a. An array of a string-literal alias takes each array method that an
   array of an enum takes. `includes` and `indexOf` compare the alias
   value. Any method that orders or compares the elements by their
   string in `node` must give the `node` result, or stay rejected: a
   `sort()` with no comparator orders by the string in `node`, so it
   stays rejected with the S014 text if it orders by the discriminant.
   *(Added 2026-10-31: the coding agent measured that `map` was
   rejected.)*
3. The runtime formatter does not change. An alias element has a string
   form on each tier already.
4. The intermediate array is the array of strings that the element count
   gives, so its cost comes from the data that the source declares.
5. Another element type keeps the existing `join` rules and texts.

### 216.2 Acceptance

1. Red first, at the pin `673b1113`: the accept entry
   `a399-join-string-alias` is rejected. The tracking note records the
   result.
2. `a399` passes `tsc`, and its output under `node` equals its golden.
   It has `join` with a separator, with no argument, an empty array, a
   template of an alias array, a receiver with a side effect that runs
   once, `map`, `filter`, `includes`, `indexOf`, and an alias with a
   C wire value (`CEnum<…>`, the `a313` shape).
3. The rejection-site total check passes. `stdlib.md` and C24 row 22
   cite this section if they name the alias element.
4. A unit test pins rule 1a for `sort()` with no comparator.
5. No golden moves, except the growth of the LIR snapshot if the entry
   adds a section.

### 216.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `673b1113..e6f03a14` gave no
CRITICAL or MAJOR finding and five MINOR findings.

1. MINOR, contrived, fixed: a `@Descriptor` default with `join` on an
   alias array gave an internal error in a function that cannot raise.
   `expression_can_raise` had no arm for a descriptor literal. A
   written `map` at that place gave the same error at the pin.
2. MINOR, fixed: the tracking table gave S014 at the pin for eight
   methods that the pin accepted.
3. MINOR, realistic, fixed: most methods of rule 1a had no corpus
   witness.
4. MINOR, realistic, fixed: the element-kind unit table had no alias
   row.
5. MINOR, realistic, open: §216.4 item 2.

### 216.4 Open

1. `ms.join(f())`, where `f` changes `ms`: the rewrite evaluates `f`
   after `map`, and `node` evaluates it before. Contrived.
2. `const r: Mode[] = ms.map((x: Mode) => x === "a" ? "z" : "a");` gives
   S100 "the initializer expects `Mode[]`, got `string[]`": `map` gives
   a callback no return context. `tsc` accepts it. A `: Mode` annotation
   on the callback is accepted.
