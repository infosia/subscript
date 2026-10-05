<!-- §164 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 164. One exit predicate, and the required arguments

*(Added 2026-10-06.)* Origin: `compiler.md` §156.3, the three defects
present before §156. The owner selected them on 2026-10-06.

Problem: the checker accepts programs that the lowering cannot build,
and it rejects programs that `tsc` accepts, at the end of a function and
at a call. Measured at `41f593bd` with `tsc` 5.9.2 (`subscript run`;
each row is the body of `function w(x: i32): i32` unless the row shows
another declaration):

| Program | `tsc` | This compiler |
|---|---|---|
| `while (true) { return x; }` | accepts | internal lowering error: "non-void function has a reachable fallthrough" |
| `for (; true; ) { return x; }` | accepts | the same internal error |
| `while (true) { if (x > 0) { return x; } }` | accepts | the same internal error |
| `while (true) { throw new Error("e"); }` | accepts | the same internal error |
| `while (true) { switch (x) { case 1: break; default: return x; } }` | accepts | the same internal error |
| `try { while (true) { return x; } } catch (e) { return 0; }` | accepts | the same internal error |
| `using r: R = new R();` then `while (true) { return x; }` | accepts | the same internal error |
| `for (;;) { return x; }` | accepts | accepts |
| `if (true) { return x; }` | accepts | S100 "not all paths return a value" |
| `if (false) { } else { return x; }` | accepts | S100, the same |
| `switch (x) { case 1: case 2: return 1; default: return 0; }` | accepts | S100, the same |
| `if (x > 0) { return 1; } else if (true) { return 2; }` | accepts | S100, the same |
| `function f(a: i32 = 1, b: i32): i32` called as `f(2)` | TS2554 | the checker accepts; internal lowering error: "missing argument `b` with no default" |
| `const h = (a: i32, b: i32 = 5): i32 => a + b;` called as `h(1)` | accepts | S100 "`the function value` expects 2 argument(s) (2 required), got 1" |
| the same `h`, called as `h()` | TS2554 | S100, the same text with "got 0" |
| `function f(a: i32, b: i32 = 5)`, `const g = f;`, `g(1)` | accepts | S100, the same text |

Three causes:

- Three parts answer "can control leave the end of these statements",
  and each derives the answer separately: the return check
  (`always_returns`), the scope-exit predicate of §101 rule 2
  (`sequence_can_fall_through`), and the lowering CFG. The lowering
  gives a loop with the condition literal `true` a false edge to its
  exit, so the CFG says that control leaves the loop. The return check
  does not use the constant condition of an `if`, or a `case` group
  with no body. §101 rule 2 does not use the closed alias of a
  `switch`, but the lowering does (`switch.exhaustive`).
- The required argument count is the number of parameters with no
  default. `tsc` counts each parameter up to the last parameter with no
  default.
- A function type has no optional parameter (C7, §155), so a call
  through a function value passes every parameter. The message does not
  state this, and the site is `TscRejects` where `tsc` accepts.

### 164.1 Rules

1. One predicate answers "can control leave the end of these
   statements". It is the predicate of §101 rule 2. The return check of
   a function with a non-void result, the scope-exit placement, and the
   §154 site facts use it. No other derivation of this answer stays in
   the checker, with one exception: the definite-assignment flow
   (`compiler/src/check/assignment_flow.rs`) models the `tsc` assignment
   analysis and keeps its own exits.
2. The predicate of rule 1 also answers "no" after a `switch` over a
   closed alias (Q32) that has a `case` for each member and no
   `default`, if each `case` cannot leave its end. The lowering already
   gives that `switch` no exit edge for an unmatched value.
3. The lowering gives a branch on the literal `true` or `false` one edge
   only: the edge that the literal takes. This applies to `if`, `while`,
   and `for`. The lowering does not lower a branch or a loop body that
   no edge reaches, as for `if (false)`.
4. The lowering compares two facts at the end of each function: the
   CFG reaches the end, and the predicate of rule 1 answers "yes" for
   the body. If the CFG reaches the end and the predicate answers "no",
   the lowering stops with an internal error that names the function
   and both facts. This applies to every function, with a void result
   or not, so every corpus entry runs the comparison. The reverse
   difference is not an error: the predicate counts a `catch` handler
   as reachable, as `tsc` does, and the lowering gives a handler no edge
   when its body cannot raise (`try { return; } catch (e) {}`).
5. The required argument count of a declaration is the number of
   parameters up to and including the last parameter with no default.
   A call with fewer arguments is rejected at the argument count site.
   That site stays `TscRejects` (TS2554). A parameter with a default
   before a required parameter takes its default at no call. Every
   derivation of the required count uses this rule, also the inference
   of a generic call.
6. A call through a function value with an argument count that is not
   the parameter count is rejected with S100. The message names the
   callee when the callee is a name, and states the rule:
   `` `h` expects 2 argument(s), got 1: a call through a function value
   passes every parameter `` (with the names and counts of the
   program). Its note states the form: a function type has no optional
   parameter (C7); pass every argument, or call a declaration with a
   default directly.
7. The site of rule 6 splits on one fact (§154 rule 2): `tsc` gives the
   callee optional parameters. The checker records the fact on a
   binding (a local or a global) that has no written type and whose
   initializer is one of these: a lambda or a function name with a
   default parameter, or a read of another binding with the fact. The
   binding records the required count of rule 5 for its source.
   - If the callee is a read of such a binding, or a lambda with a
     default parameter that is called where it is written, and the
     argument count
     is at least the recorded required count, the site is `Diverges`
     (`collisions.md` C24 row 33), with the divergence block.
   - Otherwise, the site is `TscRejects`.
7a. An assignment to a binding with the fact of rule 7 is rejected
   with S100 if the value needs more arguments than the recorded
   required count. The value needs its own required count if it is a
   lambda, a function name, or a binding with the fact, and its
   parameter count otherwise. `tsc` rejects such an assignment
   (TS2322), so the site is `TscRejects`. Example:
   `let h = (a: i32, b: i32 = 5): i32 => a + b;` then
   `h = (a: i32, b: i32): i32 => a * b;`.
8. No program changes from accepted to rejected, except a call that
   rule 5 makes too short and an assignment of rule 7a. `tsc` rejects
   each, except a rule 7a value that has no fact (164.2). No diagnostic
   changes its rule code.

### 164.2 Residual policy

The fact of rule 7 approximates the `tsc` type of a function value.
These get no fact here, and `tsc` gives them optional parameters, so the
site claims `TscRejects` where `tsc` accepts: a field initializer, a
lambda result, and an element of a container. The same applies to a
rule 7a value with no fact that `tsc` gives optional parameters: a
conditional of two lambdas with defaults, a field read, and a chained
assignment `k = h = …`. These were accepted before §164, and rule 7a
rejects them. Such a case is a missing
fact: it is fixed when it is found and does not block a phase. A site
that claims `Diverges` where `tsc` rejects is MAJOR.

### 164.3 Acceptance

1. Red first. At the contract pin, record the result of each entry:
   - Accept entry `a334`, `js-comparable: yes`, with each `tsc`-accepted
     row of the rules 1 to 4 part of the table, each in its own
     function, with output. Its golden is the `node` output.
   - Reject entry `r375`: a parameter with a default before a required
     parameter, and a call with one argument (`tsc: rejects TS2554`).
     At the pin, the checker accepts it.
   - Reject entry `r376`: a call with fewer arguments through an
     unannotated `const` binding of a lambda with a default
     (`tsc: accepts`, the C24 row 33 divergence). At the pin, the
     message and the class differ from rules 6 and 7.
2. Unit tests for rules 1 and 2: each shape of the table, and a `using`
   scope that ends in each of them, with a same-shape control that can
   leave its end (`while (x > 0)`, `if (x > 0)` with no `else`, a
   closed-alias `switch` with a `case` that ends in `break`).
3. Unit tests for rule 3: the LIR of `while (true)`, `for (; true; )`,
   `if (true)`, and `if (false)` has one edge from the branch; the
   control `while (x > 0)` has two. `while (false)` and
   `for (; false; )` with a body that reaches its end (`x += 1;`), in a
   void and a non-void function, run and give the output of `tsc`-built
   JavaScript.
4. A unit test for rule 4 builds the violating form: a function whose
   CFG reaches its end where the predicate answers "no". It shows the
   internal error. Its control has facts that agree. A second control,
   `try { return; } catch (e) {}` in a void function, is accepted.
5. Unit tests for rule 5: `f(a = 1, b)` with `f(2)` is rejected, and
   `f(1, 2)` is accepted; the control `f(a, b = 1, c = 2)` with `f(1)`
   is accepted.
6. Unit tests for rule 7, each with a same-shape control that gives the
   other class: `const h` of a lambda with a default, `const g = f`, and
   `const g2 = g`, each called with fewer arguments (`Diverges`); a
   parameter `k: (a: i32, b: i32) => i32` called as `k(1)`, and `h()`
   below the required count (`TscRejects`). Rule 7a: the assignment of
   a lambda with a required `b`, and of a binding with no fact, is
   rejected; the control assigns a lambda with a default `b`. Rule 5:
   the generic call `f<T>(a: i32 = 1, b: T)` with `f(2)` is
   `TscRejects`.
7. The §154 total test passes, and C24 row 33 has its divergence text in
   `compiler/src/divergence/`.
8. Every existing corpus entry keeps its accept or reject result. A
   reject entry whose expected text names the old message moves to the
   new message; list each one in the tracking note.
9. No existing `.expected` golden moves. Rule 3 moves the LIR text of
   each corpus function with a constant branch; list each one in the
   tracking note.

### 164.4 Open

1. A function value with a default parameter, passed where a function
   type with fewer parameters is expected (`app(f)` with
   `k: (a: i32) => i32`), is rejected with a type mismatch; `tsc`
   accepts it. The site claims `TscRejects`. A missing fact under
   164.2: an adapter that fills the default has no decided lowering.
2. A parenthesized literal condition (`if ((true))`, `while ((true))`)
   counts as constant here; `tsc` counts only a bare `true` or `false`
   (TS2366, and TS18047 for a narrowing after it). The HIR does not
   record the parentheses. Contrived.
3. Rule 7a uses one required count, but `tsc` makes each position
   optional or required. A default before a required parameter
   (`let h = f` with `f(a: i32 = 1, b: i32)`) accepts
   `h = (a: i32, b: i32): i32 => a * b`; `tsc` gives TS2322. Present
   before §164. Contrived.
4. The note and the divergence `why` of the rule 6 site both state that
   a function type has no optional parameter.
5. A constant condition built with an operator (`if (true && true)`,
   `while (x > 0 || true)`) is not constant here; `tsc` accepts a
   function that ends after it. A missing fact under 164.2. Contrived.
