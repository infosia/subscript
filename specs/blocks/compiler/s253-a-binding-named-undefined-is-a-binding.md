<!-- §253 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 253. A binding named `undefined` is a binding

*(Added 2026-11-12.)* Origin: §250.4 item 1. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-12.

Problem, measured at `6f508a61` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. A parameter or a local can have the name
`undefined`. The checker accepts the declaration, and it reads each
identifier `undefined` as the token of C7, also where a binding of that
name is in scope.

| Form | subscript | `tsc` |
|---|---|---|
| `function f(undefined: i32): void { return undefined; }` | accepted, returns nothing | TS2322 "Type 'number' is not assignable to type 'void'" |
| `function g(): void { const undefined = 3; return undefined; }` | accepted | TS2322 |
| `const k = (undefined: i32): void => undefined;` | accepted | TS2322 |
| `function n(undefined: i32): i32 { return undefined + 1; }` | S012 "`undefined` is legal only as …" | accepts |
| `if (undefined === null)` with `undefined: string \| null` | S012 "`===` compares a `null` and an `undefined` absence" | accepts |
| `` for (const undefined of [1, 2]) print(`q ${undefined}`); `` | S012 | accepts |

The first three forms break invariant 5. The check of the token reads
the syntax only (`is_undefined_token`), so a binding does not change it.

### 253.1 Rules

1. An identifier `undefined` is the token of C7 only if no binding of
   that name is in scope at the identifier. If a binding is in scope,
   the identifier reads the binding, as each other name does.
2. Rule 1 holds at each place that reads the token: a return operand
   (§195.2, §250), an absence comparison (§43), and each place that
   rejects the token (C7).
3. A program with no binding named `undefined` does not change.
4. No program that `tsc` rejects changes from rejected to accepted. The
   first three forms of the table are rejected: each returns a value of
   the binding type from a function with the result `void`.
5. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 253.2 Acceptance

1. Red first, at the pin: the reject entry
   `r499-a-binding-named-undefined-is-a-binding` with the first three
   forms is accepted, and the accept entry
   `a437-a-binding-named-undefined-is-a-binding` with the last three
   forms is rejected. The tracking note records the output.
2. After the change `r499` is rejected at each form, and `a437` passes
   `tsc` and equals `node`.
3. A unit test pins rules 1 to 3, with controls: `return undefined;`
   with no binding in a function with the result `void` stays accepted,
   and a binding in an inner scope does not change the token in an
   outer scope.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.
