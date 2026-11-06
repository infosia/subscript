<!-- §229 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 229. A callback result takes the context of the call

*(Added 2026-11-06.)* Origin: §212.4 item 5 and §216.4 item 2. The owner
selected this section on 2026-11-06.

Problem, measured at `12e01a1b` with `tsc` 5.9.2 and the debug CLI.
`type Mode = "a" | "b"`, `empty<T>(): Buf<T>`, and
`ap<T, U>(x: T, f: (v: T) => U): U`. Each form is the result of a
function with the stated return type:

| Form | Return type | subscript | `tsc` |
|---|---|---|---|
| `ms.map((x: Mode) => x === "a" ? "b" : "a")` | `Mode[]` | S100 got `string[]` | accepts |
| `ms.map<Mode>((x: Mode) => x === "a" ? "b" : "a")` | `Mode[]` | S100 got `string[]` | accepts |
| `xs.map((x: i32) => "a")` | `Mode[]` | S100 got `string[]` | accepts |
| `xs.map((x: i32) => empty())` | `Buf<P>[]` | S100 cannot infer `T` | accepts |
| `xs.flatMap((x: i32) => ["a"])` | `Mode[]` | S100 got `string[]` | accepts |
| `ap(1, (v: i32) => "a")` | `Mode` | S100 got `string` | accepts |
| `ap<i32, Mode>(1, (v: i32) => "a")` | `Mode` | accepts | accepts |
| `xs.map((x: i32) => s)`, `s: string` | `Mode[]` | S100 | TS2322 |

The checker checks the body of a callback with no context, so a literal
result widens and a generic call in the body has no candidate. §212
gives a context to a call, and the callback result is not a place that
receives it. An explicit type argument of `map` has no effect.

### 229.1 Rules

1. If the result type of a callback parameter is a type parameter `U`
   of the call, and `U` has a contextual candidate (§212.1 rule 1), the
   checker checks the expression body of the callback with that
   candidate as its context. This holds for a generic function, a
   generic method, `map`, and `flatMap`.
2. The order of §212.1 rule 2 stays. A body result that is not a
   literal is the candidate of `U`, and it comes before the context. So
   the last row of the table stays rejected.
3. An explicit type argument of `map` or `flatMap` sets `U`. The checker
   checks the body with `U` as its context, as it does for a generic
   function. At the pin the argument had no effect, so a program with an
   explicit argument can change: `xs.map<i64>((x: i32) => 2147483647)`
   gave `-2147483648` after `+ 1`, and now gives `2147483648`, as `node`
   does. `xs.map<f64>((x: i32) => x)` gave `i32[]`, and now gives S007,
   as `ap<i32, f64>(3, (v: i32) => v)` does at the pin. The tracking note
   records each. *(Amended 2026-11-06 after the Phase Review, M1.)*
4. With no explicit type argument, §212.1 rule 6 holds: no accepted
   program changes its meaning. The context decides only a `U` that gave
   a diagnostic, or a literal that failed the context.
5. A callback with a block body keeps its return type annotation rule.
   A callback with a return type annotation keeps that type.
6. Other built-in methods that take a callback keep their present rule.
   The tracking note lists each, with the `tsc` result of the same form.
7. Inference is a check-time step. The program has no run-time cost.

### 229.2 Acceptance

1. Red first, at the pin: the accept entry
   `a418-callback-result-takes-context` is rejected. The tracking note
   records the output.
2. `a418` passes `tsc`. Its output under `node` equals its golden. It
   has each accepted row of the table, a nested call in a callback
   body, a generic method, and a numeric context (`i64[]`, `f64[]`, and
   `f64` through a nested `ap`). A generic class cannot declare a
   generic method, so no generic class row reaches rule 1. *(Amended
   2026-11-06 after the Phase Review, M1 and m3.)*
3. `r485-callback-result-wider-than-context` rejects the last row of the
   table. Its header states the `tsc` result, measured.
4. A unit test pins rules 1 to 5, with a control for each: no context
   gives the present result, a non-literal body result wins over the
   context, and an annotated callback keeps its type.
5. If the checker cannot reach the context of the call when it checks
   the callback, report it and stop. Rule 1 then needs a contract change.
6. The rejection-site total check passes. §212.4 item 5 and §216.4 item 2
   cite this section. No other golden moves.
