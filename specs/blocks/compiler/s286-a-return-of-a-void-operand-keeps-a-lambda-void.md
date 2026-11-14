<!-- §286 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 286. A `return` of a `void` operand keeps a lambda `void`

*(Added 2026-11-14.)* Origin: §238.4 item 2. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `cdfd63bd` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): void` is a function and
`w1(): Promise<void>` is async.

| Form | subscript | `tsc` |
|---|---|---|
| `const h = (c: boolean) => { if (c) return v(); print("h"); };` | S100 "a lambda with a block body requires a return type annotation" | accepts |
| `const a = async (c: boolean) => { if (c) return await w1(); print("a"); };` | S100, the same | accepts |
| `const z = (c: boolean) => { if (c) { return; } return v(); };` | S100, the same | accepts |
| `function g(c: boolean): void { if (c) return v(); print("g"); }` | accepts | accepts |
| `const f = (c: boolean): void => { if (c) return v(); print("f"); };` | accepts | accepts |
| `const bad = (c: boolean) => { if (c) return v(); return 1; };` | S100, the same | accepts, the result is `number \| undefined` |

§238 rule 1 decides the result `void` from the syntax. It counts each
`return` with an operand as a value result, so a `return` of a `void`
call makes the lambda need an annotation. Under `tsc` the result of
such a lambda is `void` when each `return` operand has type `void`.

### 286.1 Rules

1. A lambda with no result annotation and a block body, in which each
   `return` with an operand has an operand of type `void`, has the
   result `void`. An async lambda of this kind has the result
   `Promise<void>`. A `return` in a nested function or lambda does not
   count, as in §238 rule 1.
2. Before its body is checked, such a lambda takes the result `void`
   (or `Promise<void>`), as a lambda of §238 rule 1 does. So §239 and
   §284 apply to it unchanged.
3. Each `return` operand of such a lambda is checked with no context.
   If one operand has a type other than `void`, the lambda gets the
   present diagnostic: S100 "a lambda with a block body requires a
   return type annotation" (`BlockLambdaReturnAnnotationMissing`). The
   program is rejected, so the result `void` of rule 2 does not reach
   an accepted program. The last row of the table stays rejected.
3a. Before the result `void` of rule 2 is taken, the checker reads the
   syntax of each `return` operand. If an operand is a form that never
   has type `void`, the lambda gets the diagnostic of rule 3 at once,
   and its type is `Error`, as at the pin. These forms are a literal,
   a template, the `undefined` token, an array or object literal, a
   unary, binary, or update operator, `new`, `this`, and a lambda or
   function expression. A parenthesis and a conditional read their
   inner operands. So a caller of the lambda gets no second
   diagnostic. The body is still checked with the result `Error`, as at
   the pin, so a diagnostic of another statement stays.
   A `return` operand of a lambda of rule 1 that has an error of its
   own, such as an unknown name, gives that error and not the
   diagnostic of rule 3. *(Amended 2026-11-14 after the Phase Review, finding
   1.)*
3b. A conditional operand whose branches are each a `void` call takes
   the `void` operand rule of §237 rule 2, as in a function with the
   result `void`. *(Amended 2026-11-14 after the Phase Review, finding
   2.)*
4. A lambda with a result annotation, or with a result from its
   context, keeps the present rule.
5. A form that the checker accepts at the pin keeps its type and its
   output.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 286.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a451-a-return-of-a-void-operand-keeps-a-lambda-void` is rejected
   with the S100 of the table. The tracking note records the output.
2. `a451` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds the first three rows of the
   table, a lambda of rule 1 that calls itself (§284), and a nested
   lambda that returns a value inside a lambda of rule 1.
3. A unit test pins rule 3: the last row of the table, a `return 1`
   alone, and a `return` of a string each keep the present diagnostic.
   A control with each `return` operand of type `void` is accepted with
   the type `(c: boolean) => void`.
3a. A unit test pins rules 3a and 3b: a module-scope
   `const f = () => { return 1; };` with a caller `const n: i32 = f();`
   gives one diagnostic, and so does a self-call in a lambda that
   returns a value. `return undefined;` gives the diagnostic of rule 3.
   `(c: boolean) => { return c ? v() : v(); }` is accepted.
4. The full gate passes. If `a451` has an async entry, the LIR text
   snapshot grows by its functions. No other golden moves.

### 286.3 Phase Review

Pending.

### 286.4 Open

None.
