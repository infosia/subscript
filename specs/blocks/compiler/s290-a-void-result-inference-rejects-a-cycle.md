<!-- §290 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 290. A `void` result inference rejects a cycle

*(Added 2026-11-14.)* Origin: §286.4 items 1, 2, and 4. On 2026-11-14
the owner told Claude to do each open item, in an order that Claude
selects.

Problem, measured at `16d23289` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `g(): i32` and `v(): void` are functions,
and `c` is a `const` of type `boolean`. Each form is at module scope.

| Form | subscript | `tsc` |
|---|---|---|
| `const a = () => { return b(); }; const b = () => { return a(); };` | accepts, `() => void` for each | TS7023 at `a` and at `b` |
| the same cycle through three lambdas `a`, `b`, `c` | accepts | TS7023 at each |
| the same cycle with `async` lambdas | accepts | TS7023 at each |
| `const a = () => b(); const b = () => a();` | S100 "initializer type depends on its own undecided type" | TS7023 |
| `const a = () => { return a(); };` | accepts | accepts |
| `const a = () => { return g(); };` and then `const x = a();` | S100 at the lambda, and a second S100 "a `void` expression is only allowed as an expression statement" at `a()` | accepts |
| `const a = () => { return c ? g() : v(); };` and then `const x = a();` | S100 at the lambda, and the same second S100 at `a()` | accepts |

The first three rows violate invariant 5: the checker accepts a program
that `tsc` rejects. The last two rows give a diagnostic at a call that
is correct, because the lambda already has a diagnostic.

### 290.1 Rules

1. An unannotated lambda whose result comes from §286 (each `return`
   operand has the type `void`) is in a cycle when a `return` operand
   reads the result of another unannotated lambda whose result is not
   decided yet. A cycle gives S100 "initializer type depends on its own
   undecided type" at each lambda of the cycle that the checker
   reaches, as §156 rule 3 gives for an expression body. This holds
   for a synchronous and for an async lambda (§287). Only a read of the
   result counts: the callee of a call in the operand. A read of the
   lambda as a value, for example an argument `after(pong)`, does not
   read its result, and `tsc` accepts it. *(Amended 2026-11-14 after
   the Phase Review, finding 1.)*
2. A `return` operand that calls the lambda itself, with no other
   lambda in the path, is not a cycle. `tsc` accepts it, and §286
   keeps it.
3. If the result of a lambda is rejected (rule 3 or rule 3a of §286,
   rule 1 here, or a result annotation that does not resolve), the
   lambda has the type `Error`. A use of the lambda gives no further
   diagnostic.
4. A form that the checker accepts at the pin, other than the forms of
   rule 1, keeps its type and its output.

### 290.2 Acceptance

1. Red first, at the contract commit: the reject entry
   `r511-mutually-recursive-void-lambdas` is accepted. The tracking
   note records the output.
2. `r511` holds the first row of the table at module scope. Its header
   states `tsc: rejects TS7023`, measured, and its expected error is
   the S100 of rule 1 at `a`.
3. A unit test pins rules 1 to 3, with an exact count of diagnostics:
   the three-lambda cycle, the async cycle, the self-call of rule 2
   (no diagnostic), and the last two rows of the table (one diagnostic
   each, at the lambda).
4. The full gate passes. No golden moves.

### 290.3 Phase Review

One fresh review. No CRITICAL finding. Red at the pin, rules 1 to 3,
and the accepted forms of rule 4 agreed with a binary built from the
pin and with `tsc`: a chain with no cycle, declared in each order; an
annotated function in a cycle; a value read in a statement; a nested
lambda; a cycle of static class fields; a mix of block and expression
bodies.

1. MAJOR, realistic, fixed: a lambda read as a value in a call
   argument of a `return` operand (`return after(pong);`) counted as a
   read of its result, so a pair of lambdas that pass each other as a
   callback got the S100 of rule 1. The pin and `tsc` accept it. Rule
   1 is amended.
2. MINOR, contrived, recorded: a cycle through a local lambda, or
   through a module `const` that is not a lambda, stays accepted. 290.4
   item 1.
3. MINOR, contrived, fixed: rule 3 applied to each lambda with a
   rejected result, not only to the results of §286. It changes
   rejected programs only. Rule 3 is amended to state it.

### 290.4 Open

1. These cycles stay accepted, and `tsc` rejects each with TS7023:
   `b` calls a local lambda `inner` that calls `a`; `a` calls
   `table[0]()` where `const table = [a];`.
2. A generic identity call of a pending lambda in an operand,
   `return id(b)();`, reads the result through a value, and is not
   counted by rule 1.
