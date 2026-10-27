<!-- §200 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 200. A switch over every enum member ends the flow

*(Added 2026-10-27.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove, and selected
this one fourth. The measurement is in
`specs/tracking/exhaustive-enum-switch.md`.

Problem, measured at the pin `409a05f5`: a function that returns from each
`case` of a `switch` over every member of a source `enum` gives S100
"not all paths return a value". `tsc` 5.9.2 accepts it, and gives
TS2366 when one member has no `case`. §42.1 says that an enum switch
has "default-free coverage no analysis can prove". The checker has
that analysis: `source_enum_switch_exhaustive` decides definite
assignment for the same switch (§158). A string-literal alias switch
already ends the return flow (§42.1).

A value that is not a member can reach a switch:
`Context.fromBytes<Wire>` of a value class with an enum field gives
the value `9` for a two-member enum at the pin (measured), and a host
can pass any integer to an exported function.

### 200.1 Rules

1. A `switch` on a source-enum discriminant with a `case` for each
   member value and no `default` ends the flow for the return-flow
   analysis of §42.1, as a `switch` with a `default` does. The member
   test is `source_enum_switch_exhaustive`, the test of §158.
2. Rule 1 applies in a function that returns a value, to a switch
   whose cases do not complete normally. Such a switch gets a
   `default` arm that traps with `unreachable-reached` at the position
   of the `switch`. A value that is not a member reaches that trap. A
   member value never reaches it, so the trap has no cost on a path
   that runs. A switch in a `void` function, or a switch with a case
   that completes normally, does not change.
2a. The arm is added only to a switch in tail position: the end of the
   function body follows the end of the switch, and no statement of
   the body runs between them. A switch followed by `return 3;` does
   not change, so a value that is not a member still reaches
   `return 3;`, as in JavaScript. *(Added 2026-10-27 after the
   implementation round: the first text added the arm to each such
   switch.)*
3. A switch that is not exhaustive keeps "not all paths return a
   value", as `tsc` keeps TS2366.
4. A `CEnum` alias switch keeps its own rules (§50).

### 200.2 Acceptance

1. Red first, at the pin `409a05f5`: `a383-exhaustive-enum-switch` gives
   S100 "not all paths return a value" ×3, and
   `t112-enum-switch-non-member-value` gives the same S100. The
   tracking note records each result.
2. `a383` passes `tsc`, and its output under `node` equals its golden.
3. `t112` traps with `unreachable-reached` at the `switch`, identically
   on the three execution forms, after it prints `before`.
4. A unit test shows that a switch with one member missing keeps the
   S100, and that a switch with a `default` adds no trap.
5. `compiler.md` §42.1 cites this section. After rule 1, a
   default-free switch with a `break` case is the only shape that
   reached the return-coverage sites, and `tsc` rejects it (TS2366).
   So `RETURNFLOWCOVERAGE`, `LAMBDARETURNFLOWCOVERAGE`, and their
   rejection sites are removed; the program gets the `tsc`-rejects
   text "not all paths return a value". *(Amended 2026-10-27 after the
   implementation round.)*
6. No golden moves.

### 200.3 Open

1. A switch at the end of a `try` block with a `finally` is not in
   tail position (rule 2a): the finalizer runs after it. The function
   keeps S100 "not all paths return a value", as at the pin. `tsc`
   accepts it, and after rule 1 the diagnostic has no divergence block.
