<!-- §161 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 161. Narrowing cost follows the live facts

*(Added 2026-10-05.)* Origin: a downstream report (subscript-typegpu, pin
`a502cf1`) gave its gate at 1.38 times the `de41409` time. The owner
asked whether the checker cost grows without a reason (2026-10-05). A
cost defect, not a surface change.

Problem: the narrowing bookkeeping of §124 and §154 does work at every
statement, assignment, condition, and local declaration. That work is
proportional to every path that a condition of the function names, not
to the narrowing facts that are live. Measured:

- The 55 downstream programs (the library modules, the program, and its
  generated support module; debug build, best of three `check_program`
  calls each): 18.36 s at `de41409`, 18.83 s at `566bbc87` (§153), 20.89
  s at `6fa57ff5` (§154), 21.39 s at `a502cf1d`. §156 to §160 add nothing
  net.
- A sample profile of the same programs at `a502cf1d`, inclusive shares
  of `check_program`: `end_shared_narrowing` 17.7 %,
  `FnCtx::shared_narrowing_paths` 14.9 %, `narrowing_paths` 13.0 %,
  `apply_narrowing_effects` 12.1 %, `register_shared_paths` 6.0 %, and
  `narrowing_helpers` 3.2 % (the shares overlap). Parsing is 16.1 %.
- One function with `n` blocks of `const oI = new P(); if (oI.a > 0) {
  s = s + oI.b; } touch();` (release CLI at `a502cf1d`): 1.87 s at `n` =
  1,000, 7.66 s at 2,000, 31.11 s at 4,000. The time grows with the square
  of `n`. The same blocks on one path (`o.a`) take 0.11 s at `n` = 4,000.
  The debug build at `de41409` takes 12.55 s at `n` = 1,000, so the class
  is older than §154.

The cause, from the code at `a502cf1d`:

- `register_shared_paths` records every path in every condition and
  every assigned target, null check or not, in
  `Scope::shared_narrowing_paths`.
- `FnCtx::shared_narrowing_paths` rebuilds a set of all recorded paths of
  all scopes at each statement, assignment, and condition operand.
  `FnCtx::declare` rebuilds a map of them at each local declaration.
- `narrowing_helpers` copies the helper set at each call.
- `end_shared_narrowing` computes the effects of each statement before it
  examines whether any fact is live.

Two more costs of the same class stop acceptance 2 after the narrowing
change (measured in the first implementation round): the statement
diagnostic suppression scans every local of every scope at each
statement list, and `capture::check` copies the whole local environment
at each branch (222 ms of a 328 ms check of `distinct` at `n` = 4,000).

### 161.1 Rules

1. The narrowing work at a point (a statement, an assignment, a condition,
   a local declaration, a scope exit, a loop head) is proportional to the
   size of that point's own expression plus the number of narrowing facts
   that are live there. It does not grow with the statements, the paths,
   or the facts elsewhere in the function.
2. If no fact is live at a point (no narrowed path, no ended shared path,
   and no shadowed narrowing in any scope), the work that ends facts at
   that point is constant. It builds no set and computes no effects.
3. A fact records whether its path is a shared location (C17) when the
   fact is established. No record exists for a path that no fact names.
4. A value that does not change during one module check (the helper set,
   the narrowing class list) is computed once per check, not per point.
5. The narrowing result does not change: every diagnostic, every
   narrowing, and every C17 end is the same as at the contract pin.
6. No step at a statement or a branch copies or scans every local of
   the function. It reads only the locals that the step needs.

### 161.2 Acceptance

1. Commit a generator, `compiler/tests/fixtures/s161_gen.py`, of three
   forms at a size `n`:
   - `distinct`: one function with the `n` blocks of the Problem;
   - `narrowed`: one function that narrows a field read to a local with a
     null check, then `n` blocks that read it, call a function, and test
     a member of a distinct local;
   - `blocks`: `n` functions of about 40 statements each: conditions on
     distinct member paths, calls, a loop, assignments, and one null check
     of a nullable field read.
2. After the fix, release builds on the same machine, best of three:
   - `distinct` and `narrowed`: the time at `n` = 4,000 is at most 5
     times the time at `n` = 1,000;
   - `distinct` at `n` = 4,000: at most 1.5 times the time of the same
     blocks on one path (Problem, 0.11 s at the pin);
   - `blocks` at `n` = 3,000: no slower than at the pin.
3. Record, with timing counters or a sample profile, the inclusive share
   of the narrowing bookkeeping (the functions that the Problem names) on
   `blocks` at `n` = 3,000, before and after.
4. Run `subscript check` on every corpus entry and every check fixture
   under `compiler/tests/` at the pin and after the fix. The diagnostics
   are identical. Record the count.
5. Unit tests, each with a control in the same shape:
   - a C17 end of a shared path whose only record is its fact;
   - a shadowed narrowing restored at a scope exit after many unrelated
     conditions;
   - a fact of `a && b` that a kill in `b` ends.
6. No corpus output or golden moves.

### 161.3 Open

None.
