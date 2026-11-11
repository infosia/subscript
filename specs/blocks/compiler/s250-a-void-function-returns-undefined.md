<!-- §250 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 250. A `void` function returns `undefined`

*(Added 2026-11-11.)* Origin: §237.4 item 3. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-11.

Problem, measured at `3f5ac035` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `tsc` accepts each form, with no error.

| Form | subscript |
|---|---|
| `function f(): void { return undefined; }` | S012 "`undefined` is legal only in `return undefined;` and in an absence comparison", why C7 |
| `function g(b: boolean): void { if (b) return undefined; print("g"); }` | S012, the same |
| `const h = (): void => { return undefined; };` | S012, the same |
| `const k = (): void => undefined;` | S012, the same |
| `async function a(): Promise<void> { return undefined; }` | S012, the same |

The message names `return undefined;` as legal, and rejects it. The
why of C7 is that `undefined` spells only a function result. In a
function with the result `void`, `return undefined;` is that result:
it is the same as `return;`. A generator return value is outside the
surface for another reason ("generator return values are not in the
decided surface"), and this section does not change it.

### 250.1 Rules

1. In a function, a method, or a lambda with the result `void`, and
   in an async function or lambda with the result `Promise<void>`,
   `return undefined;` is accepted, and it is `return;`. A constructor
   and a setter keep §237 rule 1: each operand is rejected. `tsc`
   rejects `return undefined;` there (TS2408, TS2409).
2. A lambda with the result `void` and the expression body `undefined`
   is accepted, and its body is empty.
3. Rules 1 and 2 hold with a result `void` that comes from an
   annotation or from a context (§238). The operand is the literal
   `undefined` only, through parentheses, also as a leaf of a
   conditional that §237 splits. An expression of the type
   `T | undefined` keeps its present diagnostic.
4. The S012 message states the places where `undefined` is legal:
   a result `T | undefined`, a result `void`, and an absence
   comparison.
5. No accepted program changes its meaning. No run-time cost.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 250.2 Acceptance

1. Red first, at the pin: the accept entry
   `a435-a-void-function-returns-undefined` is rejected. The tracking
   note records the output.
2. `a435` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, and an early `return undefined;` that
   skips a print.
3. A unit test pins rules 1 to 3, with a control: `return u();` with
   `u(): i32 | undefined` in a `void` function keeps its diagnostic.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves, except the LIR text snapshot by the
   `a435` block if its `main` is async.
