<!-- §304 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 304. A non-null store and a `const` global keep narrowing

*(Added 2026-11-15.)* Origin: §162.4 items 2 to 4. On 2026-11-15 the
owner told Claude to continue with the open items. The standing approval
of 2026-11-06 covers a `tsc` divergence with a known fix.

Problem, measured at `67fd8e65` with `tsc` 5.9.2 and the debug CLI.
`class A { x: i32 = 1; }`, `class H { f: A | null = null; }`, `mk()`
returns `A | null`, `other()` returns `void`, and
`const GC: A | null = mk();` is a module global. Each loop is
`for (let i: i32 = 0; i < 2; i++)`. `tsc` accepts each form. The checker
rejects each one with S011 "`A | null` may be null here".

| Form | Open item |
|---|---|
| `let a: A \| null = new A(); for (…) { a.x; const n = new A(); a = n; }` | §162.4 item 3 |
| `let a: A \| null = new A(); let c: A \| null = b; for (…) { a.x; c.x; a = c; }` with `const b: A \| null = new A()` | §162.4 item 3 |
| `h.f = new A(); for (…) { h.f.x; h.f = new A(); }` | §162.4 item 2 |
| `h.f = new A(); while (true) { h.f.x; h.f = new A(); break; }` | §162.4 item 2 |
| `h.f = new A(); for (…) { h.f.x; k.f = new A(); }` | §162.4 item 2 |
| `h.f = new A(); if (h.f !== null) { k.f = new A(); h.f.x }` | §162.4 item 2 |
| `if (GC === null) { return; } const g = (): i32 => GC.x;` | §162.4 item 4 |
| `if (GC === null) { return; } other(); GC.x` | §162.4 item 4 |

These forms stay rejected. `tsc` rejects each one with TS18047
(measured):

| Form | `tsc` |
|---|---|
| `for (…) { a.x; const n = mk(); a = n; }` | TS18047 at `a` |
| `for (…) { a.x; a = b; b = mk(); }` with `let b` | TS18047 at `a` |
| `h.f = new A(); for (…) { h.f.x; h.f = mk(); }` | TS18047 at `h.f` |
| `let GL: A \| null = mk();` then `if (GL === null) { return; } const g = (): i32 => GL.x; GL = null;` | TS18047 at `GL` |

These forms stay rejected, and `tsc` accepts them. C17 names them: a
store that can be null through another receiver (`k.f = null`) ends the
fact of `h.f`, and a module `let` global is a shared location.

Out of scope: the order of the stores in a loop (§162.4 item 1), and a
null comparison of a non-null type (§159.4 item 1, C7).

### 304.1 Rules

1. §162 rule 3a gains two non-null forms:
   a. a read of a `const` local whose initializer is a non-null form of
      rule 3a;
   b. a read of a local whose fact rule 3 keeps at the head of the same
      loop. The kept set is the largest set that is stable: start with
      each fact at the loop entry, and remove a fact while a store in
      the loop that rule 3 counts as nullable ends it.
   §162 rule 3b closes both forms: it rejects a kept fact that the
   checked body ends.
2. A store of a non-null value to a field does not end a fact of a path
   that ends in that field, through any receiver. A store of a value
   that can be null ends it, as C17 states.
   a. In the flow, the value is non-null if its checked type has no
      null.
   b. At a loop head (§162 rule 3), the value is non-null if it is a
      form of rule 3a or rule 1.
   c. §162 rule 3b also checks a store to a field: a store of a value
      that can be null to a field of a kept path is rejected with S011
      at the store, as for a local.
3. A module `const` global is not a shared location for C17. No code
   can store it, so a call, an `await`, and a store to another location
   do not end its fact. §162 rule 4 keeps its fact in a lambda body, as
   for a captured `const` local. The runtime check of §124.1 rule 3d
   does not apply to a read of it. A module `let` global and a static
   field stay shared.
4. The cost of rules 1 to 3 follows §162 rule 5. Rule 1b does not check
   a body again: it iterates over the loop summary only.

### 304.2 Acceptance

1. Red first: accept entry `a460`, `js-comparable: yes`, one function
   for each row of the first table, with output. Its golden is the
   `node` output. At the contract pin, the checker gives eight S011,
   one in each row function.
2. Reject entries for the second table, each with its measured header
   (`tsc: rejects TS18047`) and one deciding diagnostic. The S011 site
   of each one is `TscRejects` in the §154 total test.
3. Unit tests for each rule, each with a control in the same shape that
   gives the other result. Rule 1b has a chain of two locals and a
   nested loop. Rule 2 has a nullable store through another receiver
   (rejected) and a non-null store through it (accepted), in the flow
   and in a loop. Rule 2c has a test that builds the violating form, as
   §162 acceptance item 3 does for rule 3b. Rule 3 has a module `let`
   global in the same shapes (rejected).
4. C17 in `specs/blocks/collisions.md` states rules 2 and 3. Claude
   edits it in the implementation commit.
5. `compiler/tests/fixtures/s161_gen.py` forms at their acceptance
   sizes take at most 1.02 times the pin time (release, best of three).
6. No existing `.expected` golden moves. No accepted program becomes
   rejected.

### 304.3 Phase Review

Evidence: `specs/tracking/non-null-store-narrowing.md`. At the contract
pin, `a460` gives eight S011, one in each row function. `tsc` gives one
TS18047 in each of `r512` to `r515`. The largest time ratio of the
s161_gen.py forms is 1.0135 (`narrowed`, 4000).

One fresh review found two MAJOR findings, both realistic, and four
MINOR findings.

1. MAJOR: the reload declaration hash did not hold the mutability of a
   module global. A swap from `let G` to `const G` was accepted. A
   lambda from before the swap that stored `null` to `G` then gave a
   process stop with no trap, because rule 3 removes the runtime check
   of a `const` global read. Fixed: the hash holds the mutability, and
   a test destructures `hir::Global`, so a new field fails the test
   until the hash decides it. A reload test refuses each direction of
   the swap.
2. MAJOR: a store to a block-local that hides a loop local stayed in
   the loop summary, and ended the fact of the outer local. One
   program that the pin accepts became rejected. Fixed: a hidden
   binding removes the stores to its name and to the paths below it.
3. MINOR, fixed: the headers of `r512` to `r514` were the same; a test
   variable named the wrong result; the replaced intermediate field had
   no control in the same shape.
4. MINOR, recorded: item 1 of §304.4.

The first full gate failed in two tests of
`compiler/tests/narrowing_flow.rs`. Each one pinned a §162 result that
rules 1a and 2 change: a non-null field store ended the fact, and
`const c = new A(); a = c;` was nullable. The tests now pin the §304
results. A nullable store through the same receiver before `break`
stays rejected with no block: the loop summary does not use the order
of the stores (§162.4 item 1).

### 304.4 Open

1. A `static readonly` field stays a shared location. No code can store
   it after the class initializer, so rule 3 can apply to it. `tsc`
   keeps its narrowing across a call. Not measured on a program.
