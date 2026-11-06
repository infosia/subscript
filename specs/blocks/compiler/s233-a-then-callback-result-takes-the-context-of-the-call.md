<!-- §233 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 233. A `then` callback result takes the context of the call

*(Added 2026-11-06.)* Origin: §229.4 item 4. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `b58ce57d` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `type Mode = "a" | "b"`,
`v(): Promise<i32>`, `m(): Promise<Mode>`, and `empty<T>(): Buf<T>`.

| Form | Context | subscript | `tsc` |
|---|---|---|---|
| `return v().then((x: i32) => "a")` | `Promise<Mode>` | S100 got `Promise<string>` | accepts |
| `return v().then((x: i32) => empty())` | `Promise<Buf<P>>` | S100 no candidate | accepts |
| `const r: Mode = await v().then((x: i32) => "a")` | `Mode` | S100 got `string` | accepts |
| `return v().then((x: i32): Mode => "a")` | `Promise<Mode>` | accepts | accepts |
| `return m().catch((e: Error) => "b")` | `Promise<Mode>` | accepts | accepts |
| `return v().then((x: i32) => s)`, `s: string` | `Promise<Mode>` | S009 (capture) | TS2322 |
| `return v().then<Mode>((x: i32) => "a")` | `Promise<Mode>` | S013 no type arguments | accepts |

§229 gives a callback result the context of the call for `map`,
`flatMap`, and generic calls. §186.1 rule 4 gives a literal result of a
`catch` or rejection callback the type `T` or `U`, and gives a `then`
callback no context.

### 233.1 Rules

1. If `h.then(f)` or `h.then(f, r)` has the context `Promise<C>`, the
   checker checks the expression body of `f` with `C` as its context, as
   §229.1 rule 1 states. A rejection callback `r` keeps §186.1 rule 4.
1a. An async arrow callback gives a string literal result the type
   `string`, as `tsc` does: its result reaches the call through
   `PromiseLike`, and `tsc` does not keep the literal there. This holds
   for each callback of `then`, `catch`, and a rejection callback, so it
   also closes the same gap of §186.1 rule 4. A numeric literal result
   keeps the hint. *(Added 2026-11-06 after the Phase Review, M1:
   `then(async (x: i32) => "a")` with `Promise<Mode>` was accepted, and
   `tsc` gives TS2322. `m().catch(async (e: Error) => "b")` was accepted
   before §233, with the same `tsc` result.)*
2. An `await` with the context `C` gives its `then` call the context
   `Promise<C>` (§232.1 rule 3).
3. §229.1 rules 2, 4, and 5 apply: a body result that is not a literal
   comes first, no accepted program changes its meaning, and a block
   body keeps its annotation rule.
4. `then` keeps S013 for a type argument (§186). The last row stays
   rejected.
5. No run-time cost. The rule changes the checker only.

### 233.2 Acceptance

1. Red first, at the pin: the accept entry
   `a420-then-result-takes-context` is rejected. The tracking note
   records the output.
2. `a420` passes `tsc`. Its output under `node` equals its golden. It
   has the first three rows of the table, and `then(f, r)` with a
   literal result on each callback.
3. A unit test pins rules 1 to 3, with controls: no context gives the
   present result, a non-literal body result wins over the context, and
   an annotated callback keeps its type.
4. If a corpus entry or golden changes its type or output, report it and
   stop.
5. The rejection-site total check passes. §229.4 item 4 cites this
   section. `a420` has an async entry, so the LIR text snapshot grows by
   its functions. No other golden moves.

### 233.3 Phase Review

*(Added 2026-11-07.)* A fresh review of `21bc2228` and the implementation
gave one MAJOR and four MINOR findings.

1. MAJOR, realistic, fixed: an async arrow callback took the hint, so
   `then(async (x: i32) => "a")` with `Promise<Mode>` was accepted, and
   `tsc` gives TS2322. `catch(async …)` had the same gap before §233.
   Rule 1a closes it for each callback role.
2. MINOR, fixed: the `exercises` tag of `a420` differed from the other
   §186 entries.
3. MINOR, contrived, fixed: a `| null` path for the context had no
   reachable case, because `Promise<C> | null` is S011.
4. MINOR, realistic, open: §233.4.
5. MINOR, fixed: no unit test had an async callback.

### 233.4 Open

`tsc` accepts each form, and each stays rejected:

1. `async function f(): Promise<Mode> { return v(n).then((x: i32) => "a"); }`:
   the return context of an async function is `Mode`, not
   `Promise<Mode>`.
2. A nested adoption, `then((x: i32) => v(x).then((y: i32) => "a"))`,
   with `Promise<Mode>`.
3. `then((x: i32) => "a")` with `Promise<string | null>`.
