<!-- §238 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 238. A lambda with no value result is `void`

*(Added 2026-11-07.)* Origin: §237.4 items 2 and 5. Under the owner's
standing approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `7b2e8668` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): void` is a function, `w1(): Promise<void>`
and `wi(): Promise<i32>` are async, `ys` is `i32[]`, and
`each<T>(xs: T[], f: (x: T) => void): void` is generic.

| Form | subscript | `tsc` |
|---|---|---|
| `const f = () => v();` | S100 "a `void` expression is only allowed as an expression statement" | accepts |
| `const f = async () => v();` | S100, the same | accepts |
| `const f = () => { v(); };` | S100 "a lambda with a block body requires a return type annotation" | accepts |
| `w1().then(() => { v(); })` | S100, the same | accepts |
| `wi().then((x: i32) => v())` | S100 "a `void` expression is only allowed as an expression statement" | accepts |
| `return w1().then(() => v())` in a `Promise<void>` function | S100, the same | accepts |
| `each(xs, (x: i32) => v())` | S100, the same | accepts |
| `each(xs, (x: i32) => ys.push(x))` | S100 "the argument expects `(i32) => void`, got `(i32) => i32`" | accepts |
| `each(xs, (x: i32) => { v(); })` | S100 "a lambda with a block body requires a return type annotation" | accepts |
| `const f = () => { return 1; };` | S100, the same | accepts |

The divergence `BlockLambdaReturnAnnotationMissingForm` states its
reason: "A block result has no inference that agrees with `tsc` for
every program." A block body that has no `return` with an operand has
one result under `tsc`: `void`. An expression body of type `void` has
the result `void` under `tsc`. The reason does not hold for these
forms, and the last row keeps it.

A generic call gives a callback no context when the type of its
parameter names a type parameter that has no contextual candidate
(`compiler/src/check/inference.rs`). The result `void` of
`(x: T) => void` does not depend on `T`, so the context of the result
is known before inference.

### 238.1 Rules

1. A lambda with no result annotation has the result `void` when its
   body is an expression of type `void`, or a block in which no
   `return` has an operand. A `return` in a nested function or lambda
   does not count. An async lambda of this kind has the result
   `Promise<void>`. This holds with no context, in a `then`, `catch`,
   or `finally` callback, and in an argument of a generic call.
2. In a call of a generic function or a generic method, an arrow
   argument with no result annotation takes the result `void` as its
   context when the type of its parameter is a function type with the
   result `void`. §237 rule 4 then applies to its body. The parameter
   annotations of the arrow stay required (§149 rule 7).
3. A block body with a `return` that has an operand keeps the rule of
   `BlockLambdaReturnAnnotationMissingForm`. A lambda with a result
   annotation keeps that type.
4. A form that this checker accepts at the pin keeps its type and its
   output. A form of rule 1 or rule 2 was rejected at the pin.
5. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 238.2 Acceptance

1. Red first, at the pin: the accept entry
   `a425-lambda-with-no-value-result-is-void` is rejected. The tracking
   note records the output.
2. `a425` passes `tsc`. Its output under `node` equals its golden. It
   has each accepted row of the first table, a block body with a bare
   `return` in an `if`, a block body with a nested lambda that returns
   a value, a `then` chain whose `void` callback runs after a value
   callback, and a generic method with a `void` callback parameter.
3. A unit test pins rules 1 to 3. It has the last row of the table
   (rejected), a block body with a bare `return` and a `return 1`
   (rejected), and a nested lambda with `return 1` inside a block body
   with no operand (accepted, with the result `void`).
4. The rejection-site total check passes. Each site keeps a witness of
   a form that it still rejects.
5. If a corpus entry or golden changes its type or output, report it
   and stop. `a425` has an async entry, so the LIR text snapshot grows
   by its functions. No other golden moves.

### 238.3 Phase Review

*(Added 2026-11-07.)* One fresh review of the implementation. No
CRITICAL or MAJOR finding.

1. MINOR, fixed: the unit test of rule 2 had no rejected case of its
   own shape, and it did not check the type of the arrow or the
   discarded value.
2. MINOR, recorded: §238.4 items 1 and 2.

### 238.4 Open

1. A recursive lambda with no result annotation and a block body with
   no value result gives S100 "initializer type depends on its own
   undecided type": `const rec = (n: i32) => { if (n > 0) rec(n - 1); };`.
   `tsc` accepts it. The form with `(n: i32): void =>` is accepted.
   Rule 1 decides the result from the syntax, so the lambda can take
   the path of an annotated lambda. The pin also rejects this form.
   *(Closed 2026-11-07 by §239.)*
2. `() => { return v(); }` stays rejected, because rule 1 counts each
   `return` with an operand. `tsc` accepts it.
3. An async lambda in a place of a function type with the result `void`
   is rejected: `each(xs, async (x: i32) => { … })` gives
   "expects `(i32) => void`, got `(i32) => Promise<void>`". `tsc`
   accepts it. A function type that is not generic gives the same
   result at the pin.
4. A `void` callback of `catch` on a `Promise<i32>` gives S013, by the
   rule of §186. `tsc` accepts `wi().catch(() => v())`.
