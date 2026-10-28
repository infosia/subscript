<!-- §203 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 203. `do…while` runs its body before its test

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected this one third of the second group. The
measurement is in `specs/tracking/do-while.md`.

Problem, measured at the pin `0b4f11b0`: `do { … } while (c);` gives S100
"statement form outside the decided surface". `tsc` 5.9.2 accepts it,
and `node` runs it. `collisions.md` C24 row 29 lists it, and §124
acceptance 10 keeps it out. Neither gives a reason.

### 203.1 Rules

1. `do { body } while (c);` runs `body`, then evaluates `c`. If `c`
   is `true`, it runs `body` again. `c` is a `boolean` (C24 row 1).
2. `continue` in `body` goes to the evaluation of `c`. `break` in
   `body` leaves the loop. A `finally` and a `using` in `body` run on
   each exit from `body`, as in a `while` body.
3. **Definite assignment.** `body` runs at least once. A local that
   `body` assigns on every path that reaches `c` is assigned in `c`.
   It is assigned after the loop if `c` and each `break` path assign
   it. A `continue` path reaches `c`, so an assignment after a
   `continue` does not count on that path. This is the `tsc` rule.
4. **Narrowing.** After the loop, `c` is `false` on the exit through
   `c`. A narrowing that `c` gives on `false` holds after the loop if
   the loop has no `break`, as after a `while` loop.
5. **The HIR form.** The HIR carries a statement
   `DoWhile { body, cond }`. The HIR→LIR lowering writes the body
   block, then the test block, with a branch from the test back to the
   body. `continue` targets the test block. It adds no flag and no
   run-time operation that `while` does not have. Each HIR walker
   treats a `DoWhile` as its body, then its condition, with the loop
   edge from the condition back to the body.
6. `do…while` is legal wherever `while` is legal: in a function, a
   method, a lambda body, a generator, and an async function.

### 203.2 Acceptance

1. Red first, at the pin `0b4f11b0`: the accept entry `a388-do-while` is
   rejected. The reject entry `r448-do-while-continue-skips-assignment`
   fails its expected error at the pin. The tracking note records each
   result.
2. `a388` passes `tsc`, and its output under `node` equals its golden.
   It pins rules 1 to 4 and a `do…while` in a generator.
3. `r448` gives S100 "local `n` is read before assignment". `tsc`
   rejects it with TS2454.
4. A unit test pins the rule 5 lowering: the body block comes first,
   and `continue` branches to the test block. A control lowers a
   `while` loop.
5. A unit test pins rule 3 with a control: an assignment after a
   `continue` does not count.
6. `collisions.md` C24 row 29 drops `do…while`. §124 acceptance 10
   cites this section.
7. No golden moves.
