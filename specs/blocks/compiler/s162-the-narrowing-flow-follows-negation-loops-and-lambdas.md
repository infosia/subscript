<!-- §162 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 162. The narrowing flow follows negation, loops, and lambdas

*(Added 2026-10-05.)* Origin: `compiler.md` §159.4 items 2 to 4. The
owner selected them on 2026-10-05.

Problem: the narrowing flow of §124 and §159 drops facts that `tsc`
keeps at a negation, at a loop head and a loop exit, and at a lambda.
This checker rejects each read with S011. Measured at `888b68d0` with
`tsc` 5.9.2 (`class A { x: i32 = 1; }`, `mk()` returns `A | null`,
`cond()` returns `boolean`):

| Program | `tsc` |
|---|---|
| `const a = mk(); if (!(a === null)) { a.x }` | accepts |
| `const a = mk(); if (!(a !== null)) { return 0; } a.x` | accepts |
| `if (!((a = mk()) === null)) { a.x }` | accepts |
| `let e: A \| null = new A(); for (…) { s = s + e.x; e = new A(); }` | accepts |
| the same loop with `e = mk();` at the end | TS18047 |
| `let a: A \| null = null; while (true) { a = new A(); if (cond()) { break; } } a.x` | accepts |
| the same loop with the `break` before the assignment | TS18047 |
| `let a = mk(); while (a === null) { a = mk(); } a.x` | accepts |
| `let a = mk(); while (cond()) { a = mk(); } a.x` | TS18047 |
| `let a = p; for (;;) { if (a !== null) { break; } a = mk(); } a.x` | accepts |
| `const a = mk(); if (a === null) { return 0; } xs.map((v: i32): i32 => v + a.x)` | accepts |
| `const a: A \| null = new A(); const g = (): i32 => a.x;` | accepts |
| `const a = mk(); const g = (): i32 => a.x;` | TS18047 |

Out of scope: an indexed path (`xs[0] !== null`) is §163. A null
comparison of a non-null type stays rejected by C7 (§159.4 item 1).

### 162.1 Rules

1. `!e` has the facts of `e` with the true and the false facts
   exchanged. This applies at every depth (`!!e`, `!(a && b)`).
2. The facts after a loop are the facts that hold at every exit of the
   loop: the false facts of the loop condition at the head (none for a
   loop with no condition or the condition `true`), and the facts at
   each `break` that leaves the loop.
3. A fact holds at the loop head only if it holds at the entry and no
   part of the loop (the condition, the body, the update) can end it. A
   store of a non-null value to a local (the whole local, not a field
   path) does not end the fact of that local; it ends the facts of the
   paths through it (`e.f`). A store of a nullable value to the path, a
   store to a field of the same name through any receiver, and every
   other end of §124 and §159, end the fact, wherever they are in the
   loop. The loop summary does not carry the order of the stores or the
   receiver of a field store, so this rule uses neither.
3a. A store is non-null for rule 3 only if its value is one of these
   forms: a `new` expression; a non-null literal; a read of a local or a
   parameter whose type annotation is written in the source; a read of a
   field whose declaration in the class source is non-null; a read of a
   global whose declared type is non-null; a conditional or an assignment
   whose value parts are all these forms. A written type counts only if
   it names no type parameter. A parameter that takes its type from the
   context of a lambda or from its default has no written type. Every
   other value counts as nullable. The loop summary comes from a check
   that keeps every fact at the loop head, so a type that the checker
   derived there is not proven.
3b. A total check closes rule 3. After the final check of a loop body
   (in each generic instance), derive the stores of that body again
   from its checked HIR, with the checked types. If a store of a nullable
   value reaches a path whose fact rule 3 kept at the loop head, the
   program is rejected with S011 at the store, with a note that the loop
   keeps the null check made before it. If the checked body keeps every
   kept fact, the facts
   are an invariant of the loop, so the narrowing is proven. `tsc`
   rejects such a program (TS18047), so the site is `TscRejects`.
4. A lambda body starts with each fact of the point where the lambda is
   created, for a captured `const` local whose whole value the fact
   narrows. A fact of a path through a field of the local is not kept:
   the field is a shared location (C17).
5. The cost of rules 1 to 4 and 3b follows §161: the analysis visits each loop
   body a bounded number of times, and does not check a body again for
   each fact.
6. C7 states that a negation, a loop exit, and a lambda follow the
   narrowing flow of this section.

### 162.2 Residual policy

This checker approximates the `tsc` narrowing flow. A program that `tsc`
accepts and this checker rejects at the S011 site is a missing fact: it
is fixed when it is found and does not block a phase. A program that
`tsc` rejects and this checker accepts breaks invariant 5 and is MAJOR.

### 162.3 Acceptance

1. Red first: one accept entry, `a333`, `js-comparable: yes`, with every
   accepted row of the table, each in its own function, with output.
   Its golden is the `node` output. At the contract pin, the checker
   rejects it; record the diagnostics.
2. Reject entries for the rows that `tsc` rejects, each with its own
   measured header (`tsc: rejects TS18047`): the nullable store at the
   end of the loop, the `break` before the assignment, the loop
   condition that does not test the path, and the lambda without a
   null check. Each entry has one deciding diagnostic.
3. Unit tests for each rule, each with a control in the same shape that
   gives the other result. Rule 3 has a nested loop and a `continue`.
   Rule 2 has a `break` in a loop nested in the loop. Rule 3a has
   `a = b; b = null;` (rejected) and `a = new A(); b = null;` (accepted, control).
   Rule 3a has, each rejected with a same-shape accepted control: a
   contextually typed lambda parameter, a field of a generic instance,
   a parameter of type `T`, and an explicit `g<A>(…)` of a generic body
   that stores `p: T` in a loop. Rule 3b has a test that builds the
   violating form (a loop summary that keeps a fact the checked body
   ends) and shows that the program is rejected; its control keeps the
   summary true.
4. The §154 total test passes. The S011 site of each new reject entry is
   `TscRejects`.
5. `compiler/tests/fixtures/s161_gen.py` forms at their acceptance sizes,
   and a new form with a `break` and a `continue` in each loop,
   take at most 1.02 times the pin time (release, best of three; the
   noise of one form between runs is about 1 %).
6. No existing `.expected` golden moves.

### 162.4 Open

1. Rule 3 does not use the order of the stores. `for (…) { e.x; e =
   mk(); e = new A(); }` with a non-null `e` before the loop is
   rejected; `tsc` accepts it (measured 2026-10-05). The reversed order is TS18047. A missing fact under 162.2.
2. Rule 3 keeps no fact of a field path at the loop head when the loop
   stores a non-null value to a field of the same name, even through
   the same receiver: `h.f = new A(); while (true) { h.f.x; h.f = new
   A(); break; }` is rejected with the C17 block; `tsc` accepts it
   (measured 2026-10-05). A missing fact under
   162.2.
3. Rule 3a counts a value outside its forms as nullable. `a = b;` with
   `b` narrowed through the loop, and `const n = new A(); a = n;`, end
   the fact of `a`; `tsc` accepts both. A missing fact under 162.2.
4. Rule 4 keeps facts of `const` locals only. A lambda that reads a
   module `const` global narrowed before it (`if (GC === null) { return;
   } const g = (): i32 => GC.x;`) is rejected; `tsc` accepts it. A
   missing fact under 162.2.
