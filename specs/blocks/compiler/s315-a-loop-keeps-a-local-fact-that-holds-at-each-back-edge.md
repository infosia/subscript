<!-- §315 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 315. A loop keeps a local fact that holds at each back edge

*(Added 2026-11-16.)* Origin: §162.4 item 1. The owner selected it on
2026-11-16, after the diagnostics and cost items.

Problem, measured at `92baab50`. §162 rule 3 ends the fact of a local at
the loop head if the loop stores a nullable value to the local anywhere,
because the loop summary does not carry the order of the stores. `tsc`
reads the flow: the value that reaches the head again is the value at
each back edge. With `let e: A | null`, `if (e === null) { return; }`,
and `mk(): A | null`:

| Program | Body of `for (let i = 0; i < 3; i++)` | `tsc` | Checker |
|---|---|---|---|
| a | `print(\`${e.x}\`); e = mk(); e = new A();` | accepts | S011 at `e.x` |
| b | `print(\`${e.x}\`); e = new A(); if (i === 1) { e = mk(); continue; }` | TS18047 at `e.x` | S011 at `e.x` |
| c | `print(\`${e.x}\`); e = mk(); if (e === null) { e = new A(); }` | accepts | S011 at `e.x` |

Programs a and c are missing facts under §162.2.

### 315.1 Rules

1. A back edge of a loop is each point where control goes to the loop
   head again: the end of the body (then the update, if the loop has
   one) and each `continue` that targets the loop (then the update).
1a. An edge that leaves a `try` block or a `catch` block of a `try`
   with a `finally` carries the effects of that `finally` on the
   narrowing facts: the facts at the edge are the facts after the
   `finally` runs. This holds for each jump that leaves a `finally`
   statement: a `continue`, a `break` of a loop, and a `break` of a
   `switch`, at each depth of nested `try` statements, in the order
   that the `finally` blocks run. *(Added 2026-11-16 after the Phase
   Review: `try { break; } finally { e = mk(); }` gave the facts of the
   `break` statement, so the checker accepted `print(e.x)` after the
   loop, and the program stopped at run time. A `switch` `break`
   had the same defect at `92baab50`.)* A `finally` ends facts on a
   jump edge; it never establishes a fact there. The facts at the edge
   are the facts at the jump statement, minus each fact that the
   `finally` blocks end, as `tsc` does. *(Amended 2026-11-16 after the
   second Phase Review: the edge took the facts at the end of the
   `finally`, so `try { e = mk(); break; } finally { e = new A(); }`
   was accepted; `tsc` rejects it with TS18047.)*
1b. `tsc` accepts the programs of rule 1a: its flow does not apply the
   `finally` store to the jump edge (measured, TypeScript 5.9.2). The
   JavaScript program throws at the read. So the rejection is a
   divergence, and it shows a block of §79 rule 3 with a form to write
   (§307, §311). The flow form carries the cause of the end of a fact:
   a fact that a `finally` store ends on a jump edge keeps that cause
   through the joins, to the loop head, the loop exit, and the `switch`
   exit, until a store or a null check replaces the fact. The S011 at a
   read of a local whose fact has this cause selects a site of its own,
   with a divergence variant and an example: a null check after the
   target (`if (e === null) { return; }`). The flow keeps this cause as
   it keeps the loop-head and shared-location causes of §308. A join
   keeps the cause of a local only if each incoming edge where the fact
   does not hold carries the cause; if an edge ends the fact by the
   normal flow, the join has no cause. Only a null check that
   establishes the fact, or a store, replaces the cause. Each
   nullable-use site that has a loop-head variant in §308 (the member
   read S011, the argument or assignment S005, and the S100 site) has
   a variant for this cause too, and selects it from the cause.
   *(Amended 2026-11-16 after the second Phase Review: a join with an
   edge that ended the fact normally kept the cause, and `take(e)`
   gave a `TscRejects` S005; both claims were false.)* *(Added
   2026-11-16: round 2 stopped because the form did not carry the
   cause, so the S011 claimed `TscRejects` for a program that `tsc`
   accepts.)*
2. A fact of a whole local (not a field path) that holds at the loop
   entry holds at the loop head if it holds at each back edge, when the
   body and the update are checked with the fact at the head. The fact
   at a back edge is the fact of the normal flow of the checker at that
   point, with each store, each null check, and each end of §124 and
   §159 in their order.
3. Rule 2 is an induction: the fact holds at the entry, and each
   iteration that starts with it gives it to the next. So the fact is
   proven, and no total check is needed for it. §162 rule 3b keeps its
   check for each fact that rule 2 does not prove.
4. If the fact does not hold at a back edge, the loop head does not
   keep it. The checker then checks the loop again with the facts of
   §162 rule 3 and gives the diagnostics of that check only. In one
   check of a function, a loop whose assumption failed does not assume
   again: each later check of that loop, in a check of an enclosing
   loop, uses the facts of §162 rule 3 at once. So the count of checks
   of a loop body is linear in the depth of the loop, not exponential.
   *(Amended 2026-11-16 after the Phase Review: with a failed
   assumption at each level, the innermost body was checked 2^depth
   times; the check of a program with ten levels took 256 ms against
   13 ms at the pin.)*
5. A loop in a loop: the facts at the head of the inner loop come from
   rule 2 for the inner loop, in each check of the outer body.
6. Field paths (`e.f`) and the other ends of §162 rule 3 do not change.
   A store to the local still ends the facts of the paths through it.
7. No program that `tsc` rejects becomes accepted. Program b stays
   rejected.

### 315.2 Acceptance

1. Red first: accept entries for programs a and c (with output), and a
   reject entry for program b, with headers that state what `tsc` does,
   measured. Entries a and c fail at the pin.
2. Tests for rule 1: a `continue` in a nested block, a `continue` of
   an inner loop (it does not reach the outer back edge), a `while`, a
   `for…of`, and a `for` with an update that stores the local. A
   labelled statement and `do…while` are outside the subset (measured
   at `92baab50`).
3. Test for rule 5: an inner loop whose back edge stores a nullable
   value ends the fact at its own head, and the outer loop keeps it if
   the outer back edge restores it.
4. The cost of rule 4 is measured: the check time of the corpus at the
   pin and after the change, with `--test-threads=1`. The tracking
   note states it.
5. The development tier, the ship tier, and the interpreter give the
   same output for the accept entries.
6. Rule 1a: a test for each jump that leaves a `finally` (a `continue`,
   a loop `break`, a `switch` `break`, and two nested `try` statements),
   where the `finally` stores a nullable value and the code after the
   target reads the local. Each is rejected with S011. Programs q01 and
   q02 of the review are accepted and stop at run time at the change
   before this rule; q03 at the pin. A `finally` that stores a non-null
   value keeps the fact.
7. Rule 4: a test with loops nested ten deep, with a failed assumption
   at each level, counts the checks of the innermost body and asserts
   a count linear in the depth. The check time of that program is
   measured and stated.

### 315.3 Phase Review

*(Filled in after the review.)*

### 315.4 Open

None.
