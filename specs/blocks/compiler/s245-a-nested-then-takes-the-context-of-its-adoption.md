<!-- §245 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 245. A nested `then` takes the context of its adoption

*(Added 2026-11-08.)* Origin: §233.4 item 2. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `ffa2f1f0` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `Mode = "a" | "b"`, and
`v(n: i32): Promise<i32>` is async.

| Form | subscript | `tsc` |
|---|---|---|
| `async function h2(): Promise<Mode> { return v(1).then((x: i32) => v(x).then((y: i32) => "a")); }` | S100 "type mismatch: the return value expects `Mode`, got `Promise<string>`" | accepts |
| `const p: Promise<Mode> = v(1).then((x: i32) => v(x).then((y: i32) => "a"));` | S100 "type mismatch: the initializer expects `Promise<Mode>`, got `Promise<string>`" | accepts |
| the same with three levels, `… v(y).then((z: i32) => "a")))` | S100, the return value kind | accepts |

§233.1 rule 1 gives the expression body of `f` in `h.then(f)` the
context `C` when the call has the context `Promise<C>`. A `then`
callback that returns a handle adopts it (§186): the call gives the
result of that handle. So when the body of `f` is itself a `then` call,
the result of that inner call reaches the outer call, and its context
is `Promise<C>`. §233.1 rule 2 gives an `await` the same context for
the same reason. The inner call today gets `C`, which is not a handle
type, so its callback gets no context, and the literal `"a"` keeps the
type `string`.

### 245.1 Rules

1. If the expression body of a `then` callback has the context `C`
   (§233.1 rule 1), and the body is itself a `then` call, possibly in
   parentheses, the checker gives that call the context `Promise<C>`.
   §233.1 rule 1 then applies to the inner call, so the rule applies at
   each level. The same holds for each callback that has a result
   context: the rejection callback of `then(f, r)` and the callback of
   `catch` (§186.1 rule 4). Each of them adopts a returned handle, as
   the fulfillment callback does.
2. A block body keeps its annotation rule (§229.1 rule 5). A body that
   is not a `then` call keeps the context `C`.
3. No accepted program changes its meaning. A body result that is not
   a literal keeps its type first (§229.1 rule 2).
4. No run-time cost. The rule changes the checker only.

### 245.2 Acceptance

1. Red first, at the pin: the accept entry
   `a431-nested-then-takes-the-context-of-its-adoption` is rejected.
   The tracking note records the output.
2. `a431` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, and a nested `then` in a rejection
   callback and in a `catch` callback. Its `main` is async, so the LIR text
   snapshot grows by the `a431` block.
3. A unit test pins rule 1 at two and three levels, and a control: a
   nested `then` whose literal is not a member of `Mode` is rejected.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves except the LIR text snapshot, by the
   `a431` block only.
