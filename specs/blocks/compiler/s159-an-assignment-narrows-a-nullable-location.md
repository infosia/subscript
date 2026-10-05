<!-- §159 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 159. An assignment narrows a nullable location

*(Added 2026-10-05.)* Origin: `compiler.md` §154.3 item 10 (`collisions.md`
C24 row 26, C7). The owner selected it on 2026-10-04.

Problem: a null check narrows a nullable location; a non-null initializer
or assignment does not. `let a: A | null = new A(); a.x` gives S011 at
`f62ba7ad`, and `tsc` accepts it. C24 row 26 gives the reason "no
narrowing by assignment is decided". The narrowing flow that a null check
uses can carry the fact of an assignment as well.

Measured at `f62ba7ad` (`class A { x: i32 = 1; }`, `cond()` an opaque
`boolean`, `touch()` an opaque `void` call). This checker gives S011 for
every row.

| Program | `tsc` |
|---|---|
| `let a: A \| null = new A(); a.x` | accepts |
| `const a: A \| null = new A(); a.x` | accepts |
| `let a: A \| null = null; a = new A(); a.x` | accepts |
| `if (a === null) { a = new A(); } a.x` | accepts |
| `if (cond()) { a = new A(); } a.x` | TS18047 |
| `let a: A \| null = new A(); a = null; a.x` | TS18047 |
| `h.a = new A(); h.a.x` (a field) | accepts |
| `h.a = new A(); touch(); h.a.x` | accepts (C17 rejects) |
| `g = new A(); g.x` (a module global) | accepts |
| `a = new A();` in a `for` body, `a.x` after the loop | TS18047 |
| `a = cond() ? new A() : new A(); a.x` | accepts |

### 159.1 Rules

1. An initializer or an assignment whose value has a non-null type
   narrows the location to that type, as a null check does. A value of
   type `null`, or of a nullable type, ends the narrowing. An assignment
   used as a value has the type of its value, as in `tsc`, on every
   consumer and every tier: `const b = (a = new A());` gives `b` the
   type `A`.
1a. A null check of an assignment expression narrows the assignment
   target as a null check of the target does: `while ((a = next()) !==
   null) { a.x }` and `if ((a = mk()) !== null && a.x > 0)` are
   narrowed (`tsc` accepts both, measured by the Phase Review).
2. The narrowing follows the flow of the null-check narrowing: at a
   merge point the location is narrowed only if every path into it is
   narrowed; a loop body that can run zero times does not narrow the
   code after the loop.
3. A shared location (a field, a static field, a module global, a field
   of a boundary box) keeps C17: the narrowing ends at every point where
   other code can run and at a store to the same field through any
   receiver. A read after such a point stays rejected by the C17 site,
   `Diverges`, as a null-checked read is.
4. A read that rules 1–3 do not narrow is rejected by the S011 (or S100
   call) site, as at the pin. `tsc` rejects such a read (TS18047,
   TS2531), so the site is `TscRejects`, except where C17 decides it
   (rule 3).
5. The sites that §154 split on the assignment fact
   (`NullableMemberNonNullFlow`, `NullableNominalAssignmentNonNullFlow`,
   `NullableCallNonNullFlow`) leave the site table: such a read is now
   narrowed. `NullableMember`, `NullableNominalAssignment`, and
   `NullableCall` stay for rule 4, and the `Shared` variants stay for
   rule 3. A store of a nullable value to a shared location is a C17
   site too (`NullableNominalAssignmentShared`,
   `NullableAssignmentShared`). A store of a nullable value through the
   same path ends the narrowing as rule 1 does, not as C17 does, in
   every flow construct (a branch, a loop back edge, a `try` body): its
   read is `TscRejects`. C17 ends a narrowing only where other code can
   run or where a store reaches the location through another receiver.
1b. A null check of an assignment expression compares the target's
   declared type with `null`, so it is accepted when the value is
   non-null (`if ((a = b) !== null)` with `b: A`; `tsc` accepts it).
6. C24 row 26 is removed. The C7 sentence on narrowing by assignment
   states the rule of this section.

### 159.2 Residual policy

The rule 4 site is a measured claim: this checker approximates the `tsc`
narrowing flow. A program that `tsc` accepts and this checker rejects at
the rule 4 site is a missing fact: it is fixed when it is found and does
not block a phase. A program that `tsc` rejects and this checker accepts
breaks invariant 5 and is MAJOR.

### 159.3 Acceptance

1. Red first: one accept entry, `js-comparable: yes`, with every
   accepted row of the table except the C17 row, for a local, a field of
   a `const` local, and a module global; a member read, a method call, a
   call of a nullable function value, and a pass to a non-null
   parameter. Its golden is the `node` output. At the contract pin, the
   checker rejects it; record the diagnostics.
2. Reject entries for the one-branch `if`, the `null` reassignment, and
   the loop body (`tsc: rejects TS18047`).
3. Unit tests for each rule, each with a control in the same shape, and
   for C17: an assignment to a field, then a call, then a read.
4. The §154 total test passes with the merged sites.
5. No existing `.expected` golden moves.

### 159.4 Open

Items 2 to 4 move to §162 (negation, loops, lambdas) and §163 (an
indexed path).

1. `const b = (a = new A()); if (b !== null)` is rejected: `b` has the
   type `A` (rule 1), and a null comparison of a non-null type is
   rejected with the C7 block. `tsc` accepts it. Realistic, rare.
2. A non-null reassignment at the end of a loop body does not keep the
   narrowing at the loop head (`for (…) { e.x; e = new A(); }` with a
   non-null `e` before the loop). `tsc` accepts it. Present at
   `f62ba7ad` for a null check. Realistic, uncommon.
3. A lambda drops the narrowing of a `const` local that it captures
   (`const a: A | null = new A(); const f = (): i32 => a.x;`). `tsc`
   accepts it. Present at `f62ba7ad`. Contrived.
4. Missing facts found by the second Phase Review (present at `f62ba7ad`
   for a null check, rejected with no block, `tsc` accepts): `while
   (true) { a = new A(); if (c()) { break; } } a.x`, `if (!((a = mk())
   === null)) { a.x }`, and an indexed target `if ((xs[0] = mk()) !==
   null) { xs[0].x }`. Realistic, uncommon.
