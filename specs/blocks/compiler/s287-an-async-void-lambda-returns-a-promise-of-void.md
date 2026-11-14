<!-- §287 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 287. An async `void` lambda returns a `Promise<void>`

*(Added 2026-11-14.)* Origin: §286.4 item 3. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `4a1c14fa` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `w1(): Promise<void>` is async.

| Form | subscript | `tsc` |
|---|---|---|
| `const g = async (c: boolean) => { if (c) return w1(); log.push("g"); };` | S100 "a lambda with a block body requires a return type annotation" | accepts, `Promise<void>` |
| `const k = async () => { return w1(); };` | S100, the same | accepts, `Promise<void>` |
| `async function f(): Promise<void> { return w1(); }` | accepts | accepts |
| `const h = async (): Promise<void> => { return w1(); };` | accepts | accepts |

§286 rule 1 accepts a `return` operand of type `void` only. In an async
function, a `return` of a `Promise<void>` operand gives the result
`Promise<void>`, and the checker accepts it when the result is
annotated.

### 287.1 Rules

1. In an async lambda of §286 rule 1, a `return` operand of type
   `Promise<void>` counts as an operand of type `void`. The lambda has
   the result `Promise<void>`. The `return` takes the path that it takes
   in an async lambda with the annotation `Promise<void>`, so the
   operand is awaited the same way.
2. A non-async lambda keeps §286: `() => { return w1(); }` stays
   rejected with the diagnostic of §286 rule 3.
3. A `return` operand of type `Promise<T>` with `T` other than `void`
   gives the diagnostic of §286 rule 3.
4. A form that the checker accepts at the pin keeps its type and its
   output.

### 287.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a452-an-async-void-lambda-returns-a-promise-of-void` is rejected
   with the S100 of the table. The tracking note records the output.
2. `a452` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds the first two rows of the
   table, and a value of the lambda kept in a `const` typed
   `Promise<void>` and then awaited.
3. A unit test pins rules 2 and 3: `() => { return w1(); }` and
   `async () => { return wi(); }` with `wi(): Promise<i32>` each give the
   diagnostic of §286 rule 3 once.
4. The full gate passes. `a452` has an async entry, so the LIR text
   snapshot grows by its functions. No other golden moves.

### 287.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. Each probe gave the
same output under subscript and `node`, and `tsc` accepted it: a
`try`/`finally`, a stored handle, a conditional of two calls, a nested
async lambda, a §284 self-call, and the deferred module-scope path.

1. MINOR, realistic, fixed: for an optional-chain operand such as
   `return o?.m();`, the branch dropped the statement that the operand
   check gave, and a second diagnostic (S013) followed. The program now
   gives one diagnostic.
2. MINOR, contrived, fixed: an arm that could not run pushed a value
   `return` into a `void` frame. It is removed.
3. MINOR, fixed: a doc comment did not cite rule 1.

### 287.4 Open

None.
