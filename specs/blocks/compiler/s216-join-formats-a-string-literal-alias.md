<!-- §216 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 216. `join` formats a string-literal alias

*(Added 2026-10-31.)* Origin: §204.3 Open item 1. On 2026-10-31 the
owner asked for the open work in priority order. The measurement is in
`specs/tracking/join-string-alias.md`.

Problem, measured at `673b1113`: with `type Mode = "on" | "off"` and
`ms: Mode[]`, `ms.join(",")` gives S014 "`Mode` elements are not
interpolatable (Q22)", and `` `${ms}` `` gives the same text. A single
`` `${m}` `` with `m: Mode` is accepted. `tsc` 5.9.2 accepts each form,
and `node` prints `on,off`.

### 216.1 Rules

1. `xs.join(s)` with `xs: A[]`, where `A` is a string-literal alias,
   is `xs.map((x: A): string => `${x}`).join(s)`. `xs` evaluates once.
   `join()` with no argument uses `","`, as for another element type.
2. `` `${xs}` `` follows, because §204.1 rule 2 gives it the `join`
   form.
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
   once, and an alias with a C wire value if `@CEnum`-style wire values
   have a corpus form under `tsc`.
3. The rejection-site total check passes. `stdlib.md` and C24 row 22
   cite this section if they name the alias element.
4. No golden moves, except the growth of the LIR snapshot if the entry
   adds a section.
