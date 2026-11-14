<!-- §284 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 284. A local lambda calls itself

*(Added 2026-11-14.)* Origin: §239.4 item 2. The owner selected the
form on 2026-11-14 (option A: a self-call calls the running closure).

Problem, measured at `c346c4b1` with the debug CLI. `tsc` accepts each
form, and `node` runs it.

| Form, in a function body | subscript |
|---|---|
| `const loc = (k: i32): void => { print(\`${k}\`); if (k > 0) loc(k - 1); };` | S100 "`loc` is read before its declaration in this block" |
| the same at module level | accepts, prints `2 1 0` (§239) |
| `function inner(k: i32): void { ... inner(k - 1); }` in a function body | accepts |

The cause, read in the code. A lambda copies each capture when it is
evaluated (§274). When the lambda of `loc` is evaluated, `loc` has no
value yet, so the lambda cannot capture it. The lookup rejects the
read (`compiler/src/check/lookup.rs`, site
`BlockPendingReadWithoutProgramShadow`, divergence `DeclarationScope`,
collision C14). A module-level lambda refers to its global and captures
nothing, so §239 accepts it.

### 284.1 Rules

1. In the body of a lambda that is the whole initializer of a local
   `const`, a call whose callee is the name of that `const` is a
   self-call. A self-call calls the running closure: the same function
   with the same environment. The lambda does not capture its own
   name.
2. Rule 1 applies when the type of the lambda is decided before its
   body is checked: a result annotation, or a block body that §238
   rule 1 decides (§239 rule 1). A lambda whose result comes from an
   expression body that calls itself stays rejected, as at module
   level.
3. Rule 1 applies to a synchronous and to an `async` lambda. A
   self-call of an `async` lambda returns its `Promise`, as any call
   of it does.
4. A self-call costs no allocation and no capture. If the lambda owns
   its environment (§274 rule 1), the self-call passes that
   environment. A self-call does not make the lambda escape.
5. A self-call in a lambda nested inside the body of the lambda stays
   rejected. The nested lambda would capture the name.
6. These forms stay rejected with the present site and diagnostic:
   a read of the name that is not the callee of a call (for example
   `arr.push(loc)` or `const g = loc;`), and two local lambdas that
   call each other.
7. The development tier, the ship tier, and the reference interpreter
   give the same output for a self-call.

### 284.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a449-a-local-lambda-calls-itself` is rejected with the S100 of the
   table. The tracking note records the output.
2. `a449` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds: the form of the table; a
   self-call with no result annotation and a `void` block body; a
   lambda that captures another local and calls itself; an `async`
   lambda that awaits its self-call; a lambda that escapes (it is
   returned and called later) and calls itself, so it owns its
   environment (it captures a `const` copy of a parameter, by §274); a self-call inside a loop of the body.
3. Reject entries pin rules 5 and 6, each with its present diagnostic:
   `r508` a nested lambda that calls the outer name, `r509` a read of
   the name as a value, `r510` two local lambdas that call each other.
   Each reject entry header states what `tsc` does, measured.
4. A unit test of the checker shows that the lambda of a self-call
   captures nothing (its capture list is empty) when the lambda has no
   other capture.
5. The full gate passes. `a449` has an `async` entry, so the LIR text
   snapshot grows by its functions. No other golden moves.

### 284.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. It ran probe programs
on the three tiers and compared them byte for byte: shadowing by a
parameter, an inner block, a nested function, a `catch` and a `for`
binding; captures of a string, a class reference, `this`, and a
fixed array; a generic function; `try`/`finally` with a throw; nested
self-calls; an escaping `async` lambda dropped while its self-call
frames wait; self-call promises stored, fanned out, and passed to
`TaskGroup.add` and `Promise.all`. Each agreed. Each form outside
rule 1 keeps its rejection. Three MINOR findings, recorded:

1. Realistic: a self-call cannot omit a defaulted argument. It gets
   S100 "a call through a function value passes every parameter".
2. Realistic: rule 2 does not count a result that the annotation of
   the `const` decides. `type F = (k: i32) => i32; const fact: F =
   (k) => { ... fact(k - 1) ... };` gets the S100 of the table. `tsc`
   accepts it.
3. Contrived: a warning that names a self-call result renders the
   callee as `…`.

### 284.4 Open

1. A read of a local lambda's own name as a value stays rejected
   (rule 6). Its acceptance needs an escape rule for the running
   closure value.
2. Two local lambdas that call each other stay rejected (rule 6).
3. A self-call that omits a defaulted argument is rejected (§284.3
   item 1).
   *(Selected 2026-11-14 for §288.)*
4. A result decided by the annotation of the `const` does not count
   for rule 2 (§284.3 item 2).
   *(Closed 2026-11-14 by §285.)*
5. A warning renders a self-call callee as `…` (§284.3 item 3).
