<!-- §237 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 237. A `void` function returns a `void` operand

*(Added 2026-11-07.)* Origin: §236.4 item 1 and `collisions.md` C21.
Under the owner's standing approval of 2026-11-06 for `tsc` divergences
with a known fix.

Problem, measured at `ea723f61` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): void` and `g(x: i32): void` are
functions, `work(): Promise<void>` is async, and `xs` and `ys` are
`i32[]`.

| Form | subscript | `tsc` |
|---|---|---|
| `function f(): void { return v(); }` (`r338`) | S100 "a `void` expression is only allowed as an expression statement" | accepts |
| `const f = (): void => v();` | S100, the same | accepts |
| `async function f(): Promise<void> { return await work(); }` | S100, the same | accepts |
| `async function f(): Promise<void> { return work(); }` | S100 "a `void` function cannot return a value" | accepts |
| `xs.forEach((x: i32) => g(x))` | S100 "a `void` expression is only allowed as an expression statement" | accepts |
| `xs.forEach((x: i32) => ys.push(x))` | S100 "the lambda body expects `void`, got `i32`" | accepts |
| `const cb: (x: i32) => void = (x: i32) => ys.push(x)` | S100, the same | accepts |
| `function f(): void { return 3; }` | S100 | TS2322 |
| `const f = (): void => 3;` | S100 | TS2322 |
| `async function f(): Promise<void> { return 3; }` | S100 | TS2322 |
| `const cb: (x: i32) => void = (x: i32): void => ys.push(x)` | S100 | TS2322 |
| `const cb: (x: i32) => void = named` with `named(x: i32): i32` | S100 | accepts |

C21 states that a `void` value is never an operand. A `return` of a
`void` operand does not read a value: in `node`, `return v()` calls `v`
and returns `undefined`, as `v(); return;` does. The rejection has no
reason that holds for these forms. The lowering is known: the statement
form of the call, then a bare `return`.

An unannotated lambda whose result takes `void` from its context has,
under `tsc`, the type of its body. That type is assignable to a `void`
result, so `tsc` accepts a value. The caller of a `void` callback does
not read the result, so the lowering is known: evaluate the operand and
discard it.

A function value with a value result, in a place of a function type with
a `void` result, needs a function that discards the result. That is a
new form, and this section does not add it (§237.4).

### 237.1 Rules

1. A function has a `void` result when its result is `void`, declared
   or from the context, or when it is async with the result
   `Promise<void>`. A constructor and a setter are not such functions:
   `tsc` rejects a `return` of an operand there (TS2408, TS2409), so
   their present S100 stays.
2. In a function with a `void` result, `return e` is accepted when `e`
   has the type `void`. Its meaning is `e; return;`. An expression body
   of an arrow is such a return. The type of `e` is its type as an
   expression, not the form of `e` as a statement: an assignment has
   the type of its value, and an optional chain has the type of its
   member or its call, and `(e)` has the type of `e`. A leaf of `e` is
   as in §236 rule 3, and a
   conditional splits into one return per leaf, so
   `return b ? v() : v()` is accepted.
3. In an async function or an async arrow with the result
   `Promise<void>`, `return e` is accepted when `e` has the type
   `Promise<void>`. Its meaning is the adoption of §236 rule 2, with no
   value: the function completes with no value, or with the exception
   of the handle. §236 rules 2a, 3, and 4 apply. A mixed leaf splits as
   in §236 rule 3, so `return b ? work() : v()` is
   `if (b) return work(); else return v();`.
4. A lambda that is not async, with no result annotation, whose result
   takes `void` from its context accepts `return e` and an expression body with a value
   `e` of any type. The meaning is `e; return;`. The rules of an
   expression statement apply to `e`: a dropped handle gives S013.
5. A function with a declared `void` result, a declared
   `Promise<void>` result, or an async lambda whose result takes
   `Promise<void>` from its context, keeps the S100 for a value
   operand. `tsc` rejects these forms. Examples:
   `return ys.length = 0`, `return s?.get()` with `get(): i32`, and
   `const cb: () => Promise<void> = async () => 3`.
6. `yield e` with a `void` operand is unchanged (C21).
7. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 237.2 Acceptance

1. Red first, at the pin: the accept entry
   `a424-void-function-returns-a-void-operand` is rejected. The
   tracking note records the output.
2. `a424` passes `tsc`. Its output under `node` equals its golden. It
   has each accepted row of the first table, a `return` of a `void`
   operand inside `try`/`finally` with the order of output equal to
   `node`, an adopting `return work()` with a ticker task that shows
   the order of turns for a settled and a pending handle, a `work` that
   throws and a caller that catches it, a mixed leaf, and a
   contextual `void` lambda with a block body that returns a value.
3. `r338` is retired, and its form moves to `a424`. `collisions.md`
   C21 states the forms of rules 2 to 4 as accepted.
4. A unit test pins rules 2 to 6. It has a declared `void` result with
   a value operand (rejected), a contextual `void` lambda that drops a
   handle (S013), and a `yield` of a `void` operand (rejected).
5. The rejection-site total check passes. If a site keeps no form, the
   site, its witness, and its divergence go. Each remaining site keeps
   a witness of a form that it still rejects.
6. If a corpus entry or golden changes its type or output, report it
   and stop. `a424` has an async entry, so the LIR text snapshot grows
   by its functions. No other golden moves.

### 237.3 Phase Review

*(Added 2026-11-07.)* Three fresh reviews of the implementation. The
contract gained the present rule 1 (constructor and setter), the type
of `e` as an expression in rule 2, the split of `void` leaves, the
limit of rule 4, and the examples of rule 5.

1. MAJOR, realistic, fixed: the decision read the statement form of
   `e`, so a declared `void` function accepted `return ys.length = 0`
   and `return s?.get()`. `tsc` rejects both. Rule 2: the type of `e`
   as an expression decides, in one place.
2. MAJOR, realistic, fixed: an async lambda with the context
   `() => Promise<void>` accepted `async () => 3`. `tsc` rejects it.
   Rule 4 is for a lambda that is not async.
3. MAJOR, realistic, fixed: a constructor and a setter accepted
   `return v()`. `tsc` rejects both (TS2408, TS2409). The frame of the
   checker now carries the fact (rule 1).
4. MAJOR, realistic, fixed: a parenthesized `void` operand, such as
   `return (v())`, was rejected. `(e)` has the type of `e`.
5. MINOR, fixed: a conditional of `void` leaves was rejected; a
   rejected return gave a second S013, or one S100 per leaf; doc
   comments; the C21 revision line; the citation of the pin.
6. MINOR, recorded: in a `Promise<void>` function,
   `return a ? wi() : wi()` with `wi(): Promise<i32>` gives one
   diagnostic per leaf. `tsc` also gives one per leaf. A nested
   conditional repeats the inner diagnostics.
7. MINOR, recorded: the operand of a `void` return does not reach the
   `TaskGroup` and `void[]` position checks. Each operand that those
   checks reject is not `void`, so rule 5 rejects it with the general
   S100.
8. MINOR, recorded: in a constructor, `return v()` gives the site
   `VoidExpressionValue`, whose note says that `tsc` accepts the form.
   `tsc` rejects it (TS2409). The behaviour is the same as at
   `ea723f61`.

### 237.4 Open

1. A function value with a value result in a place of a function type
   with a `void` result stays rejected: `const cb: (x: i32) => void =
   named`, and `xs.forEach(named)`. `tsc` accepts both. A function that
   discards the result is a new form.
   *(Closed 2026-11-07 by §240.)*
2. A lambda with no result annotation and no context, whose body has
   the type `void`, stays rejected: `const f = () => v();` and
   `const f = async () => v();`. `tsc` accepts both. The binding has
   the type of the lambda, and its result type is `void`.
   *(Closed 2026-11-07 by §238.)*
3. `function f(): void { return undefined; }` stays rejected with the
   diagnostic of `collisions.md` C7. `tsc` accepts it.
4. In an async function with the result `Promise<void>`,
   `return s?.am()` with an async `am(): Promise<void>` stays rejected.
   `tsc` accepts it. The operand is a `Promise<void> | undefined`,
   which is not a handle of rule 3.
5. A lambda with no result annotation gets no `void` context from a
   `then` callback (§233 gives a literal hint only) or from a generic
   parameter whose `T` is not inferred. So
   `return w1().then(() => v())` and
   `each(xs, (x: i32) => g(x))` with
   `each<T>(xs: T[], f: (x: T) => void)` stay rejected. `tsc` accepts
   both. The annotated form `(): void => v()` is accepted.
   *(Closed 2026-11-07 by §238.)*
