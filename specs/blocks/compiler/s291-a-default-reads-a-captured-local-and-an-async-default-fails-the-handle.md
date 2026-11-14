<!-- §291 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 291. A default reads a captured local, and an async default fails the handle

*(Added 2026-11-14.)* Origin: §288.4 items 1, 2, and 3. On 2026-11-14
the owner told Claude to do each open item, in an order that Claude
selects.

Problem, measured at `16d23289` with `tsc` 5.9.2, the libraries of the
project, the debug CLI, and `node` 24 with type stripping. `fail(): i32`
throws `Error("failed")`.

| Form | subscript | `tsc` and `node` |
|---|---|---|
| `const k: i32 = 5; const cap = (x: i32 = k + 1): i32 => x;` in a function body, and a print of `cap()` | S100 "a named lambda default cannot read captured local `k`", at the default | accepts; prints `6` |
| `async function af(x: i32 = fail()): Promise<void> {}`, then in a `try`: `const p = af(); print("after call"); await p;` with `catch` printing `caught` | prints `caught` | prints `after call`, then `caught` |
| the same with `const al = async (x: i32 = fail()): Promise<void> => {};` and `al()` | prints `caught` | prints `after call`, then `caught` |

In TypeScript, a default is evaluated in the scope of the function, so
it reads the names that the function captures. An async function
evaluates its defaults in its body, so a throw of a default fails the
returned promise, and the caller continues. The checker evaluates a
default at the caller (§57, §288 rule 2). Two consequences follow: a
captured local is not in scope, and a throw leaves the caller.

### 291.1 Rules

1. A default of a lambda can read a local that the lambda captures. The
   lambda captures each local that its defaults read, by the capture
   rules of its body (§181; a capture of a `let`, a `var`, or `this`
   stays S009). A named lambda call that omits the argument evaluates
   the default at the caller and reads the captured local from the
   environment of the called lambda value, not from the scope of the
   call. This replaces §288 rule 3. The S100 of §288 rule 3 and its
   divergence block go.
2. A named lambda call that passes every argument does not evaluate the
   default. Its output does not change.
3. If a default of an `async` callee throws when a call evaluates it,
   the call does not run the callee. The call gives a handle that fails
   with the exception, and the caller continues with the next
   statement. This holds for an `async function` declaration (§57) and
   for a named lambda call of an async lambda (§288). The defaults
   before the throwing default run once, in order, at the call, as in
   `node`.
3a. If no default throws, the call is a direct call of the callee, with
   the order of callbacks that it had at the pin. Only the path of a
   throw makes the failed handle. *(Amended 2026-11-14 after the Phase
   Review, finding 2.)*
4. A default of a synchronous callee that throws keeps its present
   behavior: the exception leaves the call.
5. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 291.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a455-a-default-reads-a-captured-local-and-an-async-default-fails-the-handle`
   is rejected with the S100 of the table. The tracking note records
   the output.
2. `a455` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds: the first row; a captured
   string and a captured class reference read by a default; a default
   that reads a capture and a parameter before it; an omitted default
   at a call in a nested lambda and at a self-call; the second and the
   third row of the table, with a default before the throwing default
   that prints.
3. A unit test pins rule 1 with a capture of a `let` (one S009) and
   rule 4 with a synchronous callee (the exception leaves the call).
4. The full gate passes. `a455` has an async entry, so the LIR text
   snapshot grows by its functions. The reject entries and the unit
   tests of §288 rule 3 change to rule 1. No other golden moves.

### 291.3 Phase Review

One fresh review. No CRITICAL finding. Rule 1 (captures of an `i32`, a
string, a class reference, a value type, and arrays; a call in the same
function, in a nested lambda that shadows the capture, and in a
self-call; generic instances), the ownership of 50 iterations with
`Context.collect()`, rule 3 on its throw path, rule 4, and hot reload
agreed on the three tiers and `node`.

1. MAJOR, realistic, fixed: an async callee with a class or array
   argument, or a local receiver, and an omitted default that can
   throw gave an internal error "read-through operand has no lifetime
   site" on each tier. The pin ran these programs.
2. MAJOR, realistic, fixed: a call that omits a default that can throw
   went through a forwarding frame that awaited the callee. If the
   default did not throw, the callbacks ran one turn late against
   `node` and the pin. Rule 3a.
3. MINOR, realistic, fixed: a test ran `a455` on the three tiers a
   second time and stated no cost (core principle 15).
4. MINOR, realistic, fixed: a verifier clause compared two facts from
   one record (core principle 9).
5. MINOR, contrived, fixed: each async named lambda call that omits an
   argument walked the whole HIR.

A second review checked the form that fixed findings 1 and 2. No
CRITICAL or MAJOR finding.

6. MINOR, realistic, fixed: `await h()` of an async function or method
   whose default throws let the exception leave the call, so the
   `catch` ran one turn before `node`. The lambda form was correct.
7. MINOR, contrived, fixed: the verifier clause of a default helper
   did not compare the closure operand with its lambda.

### 291.4 Open

1. A captured `var` is S100 before the capture rule, not S009 (rule 1).
   The S100 of a `var` declaration is older than this section.
