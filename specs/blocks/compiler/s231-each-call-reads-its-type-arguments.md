<!-- §231 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 231. Each call reads its type arguments

*(Added 2026-11-06.)* Origin: §230.4 items 1 and 3. The owner selected
this section on 2026-11-06.

Problem, measured at `52e48d5f` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. Each program below is accepted, and `tsc`
rejects it, against invariant 5. `class B { get(): i32 }`,
`class Reg { static f: (x: i32) => i32 }`, and `type Mode = "a" | "b"`.

| Form | subscript | `tsc` |
|---|---|---|
| `b.get<i32>()`, `get` not generic | accepts | TS2558 |
| `const f = (x: i32): i32 => x; f<i32>(1)` | accepts | TS2558 |
| `Reg.f<i32>(1)` | accepts | TS2558 |
| `xs.reduce((acc: Mode, x: i32) => { return "b"; }, "a")`, result `Mode` | accepts | TS2322, TS2769 |

These neighbours agree with `tsc` at the pin:

| Form | subscript | `tsc` |
|---|---|---|
| `g<i32>()`, `function g(): i32` | S100 "`g` is not generic" | TS2558 |
| `B.s<i32>()`, a static method that is not generic | S100 "static method `B.s` is not generic" | TS2558 |
| `id<i32, i32>(1)`, `id<T>` | S100 "expects 1 type argument(s), got 2" | TS2558 |
| `await b.get<i32>()` | S100 "method `get` is not generic" | TS2558 |
| a `reduce` block that returns `"b"` or `acc` | accepts | accepts |

§230 rule 3 covers built-ins only. A call of a user method or a function
value has no check of its list. §230 rule 2 keeps a block body out of
rule 1, and `tsc` widens a literal that a block returns in the same way.

### 231.1 Rules

1. §230 rule 3 applies to each call. A type argument list is accepted
   only if the checker reads each entry. Each other list is rejected
   with the diagnostic of a named function: "`{name}` is not generic",
   or "expects N type argument(s), got M".
2. The check runs through the helper of §230 rule 3. A user call has
   the same check as a built-in call, so §230.1 rule 3 has no exempt
   path.
3. §230 rule 1 applies to a callback with a block body. The result
   values of the body are the expressions of its `return` statements.
   Rule 1 fires if each result value is the same string literal, as it
   does for the branches of a conditional expression.
4. §230 rule 1a applies to a block body too.
5. A call that already has a diagnostic gets no second diagnostic for
   its list.
6. No run-time cost. The rules change the checker only.

### 231.2 Acceptance

1. Red first, at the pin: `r488-user-call-unread-type-argument` has the
   `b.get<i32>()` row, and `r489-reduce-block-literal-result` has the
   block row. Each is accepted at the pin. The tracking note records the
   output.
2. Each reject entry states its `tsc` result, measured. The function
   value row and the `Reg.f` row are unit tests.
3. Each neighbour of the second table keeps its result. A unit test pins
   each, with a user generic function, a generic static method, and a
   generic instance method with the right count as accepted controls.
4. If a corpus entry, an example, or a golden uses a form that a rule
   rejects, report it and stop.
5. The rejection-site total check and `generic_tsc_matrix` pass, and the
   omission count of the matrix does not rise. §230.4 items 1 and 3 cite
   this section. No golden moves.
