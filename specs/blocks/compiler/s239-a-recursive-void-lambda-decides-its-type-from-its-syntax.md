<!-- §239 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 239. A recursive `void` lambda decides its type from its syntax

*(Added 2026-11-07.)* Origin: §238.4 item 1. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `00b8b7b2` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI:

| Form | subscript | `tsc` |
|---|---|---|
| `const rec = (n: i32) => { print(\`r${n}\`); if (n > 0) rec(n - 1); };` | S100 "initializer type depends on its own undecided type" | accepts |
| `const ping = (n: i32) => { if (n > 0) pong(n - 1); };` and `const pong = (n: i32) => { print(\`p${n}\`); ping(n); };` | S100, the same, at `pong` | accepts |
| `const arec = async (n: i32) => { if (n > 0) { await arec(n - 1); } };` | S100, the same | accepts |
| the first row with `(n: i32): void =>` | accepts | accepts |

A lambda with a result annotation takes a deferred path while the
checker decides the type of an initializer: its type comes from its
syntax, and its body is checked later
(`compiler/src/check/expr/lambda.rs`). §238 rule 1 also decides the
result `void` from the syntax alone, with no check of the body. The
deferred path does not take that lambda, so a reference to the binding
in the body is a cycle.

### 239.1 Rules

1. A lambda whose result §238 rule 1 decides from a block body with no
   `return` that has an operand takes the deferred path of a lambda
   with a result annotation. Its type is decided before its body is
   checked, so its body can refer to the binding that holds it.
2. A lambda whose result comes from an expression body is unchanged:
   the type of the body decides it.
3. A form that this checker accepts at the pin keeps its type and its
   output.
4. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 239.2 Acceptance

1. Red first, at the pin: the accept entry
   `a426-recursive-void-lambda` is rejected. The tracking note records
   the output.
2. `a426` passes `tsc`. Its output under `node` equals its golden. It
   has each accepted row of the first table, and a recursive lambda
   that writes a module-level `let` and calls a module function.
3. A unit test pins rule 1 and rule 2. Its control is a recursive
   lambda with an expression body, which stays rejected with the same
   diagnostic.
4. If a corpus entry or golden changes its type or output, report it
   and stop. `a426` has an async entry, so the LIR text snapshot grows
   by its functions. No other golden moves.

### 239.3 Phase Review

*(Added 2026-11-07.)* One fresh review of the implementation. No
CRITICAL or MAJOR finding. A `check` of each corpus entry with a binary
of the pin and a binary of the change differs only at `a426`.

1. MINOR, recorded: an error in the body of a deferred lambda is
   reported after the errors of its initializer, as for a lambda with a
   result annotation. The set and the count of the diagnostics are the
   same. For `const m = [(n: i32) => { const z: i32 = "s"; }, 5];` the
   pin reports 1:45, then 1:54; the change reports 1:54, then 1:45.

### 239.4 Open

1. An expression body whose leaves are each `void`, with no context,
   stays rejected: `const f = (n: i32) => n > 0 ? g(n) : print("e");`
   with `g(n: i32): void`. `tsc` accepts it.
   *(Closed 2026-11-12 by §258.)*
2. A local lambda that calls itself stays rejected with S100 "`loc` is
   read before its declaration in this block":
   `const loc = (k: i32) => { if (k > 0) loc(k - 1); };` in a function
   body. `tsc` accepts it. Its rule is not a rule of this section.
   *(Closed 2026-11-14 by §284 for a self-call.)*
