<!-- §292 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 292. An async `void` `catch` callback gives a `Promise<void>`

*(Added 2026-11-14.)* Origin: §289.4 item 3. On 2026-11-14 the owner
told Claude to do each open item, in an order that Claude selects. On
the same day the owner decided that §289.4 items 1 and 2 stay rejected:
each one needs a nullable scalar type, which C7 does not have.

Problem, measured at `40989f76` with `tsc` 5.9.2, the libraries of the
project, the debug CLI, and `node` 24 with type stripping. `bad():
Promise<i32>` is async and throws `Error("bad")`, and `note(m: string):
Promise<void>` is async and prints `m`.

| Form | subscript | `tsc` and `node` |
|---|---|---|
| `await bad().catch(async (e: Error) => { print(e.message); });` | S013 "a `catch` callback must return `i32`, the value type of the handle; it returns `Promise<void>`" | accepts; prints `bad` |
| `await bad().catch(async (e: Error) => { await note("n"); });` | S013, the same | accepts; prints `n` |
| `await bad().catch((e: Error) => note(e.message));` | S013, the same | accepts; prints `bad` |

§289 rule 1 accepts a `catch` callback with the result `void`. A
callback with the result `Promise<void>` is adopted (§186 rule 1), so
the adopted value is `void`, and `tsc` gives the call the same type
`Promise<T | void>`.

### 292.1 Rules

1. On a receiver `Promise<T>` with `T` other than `void`, a `catch`
   callback with the result `Promise<void>` (an async lambda of §238
   rule 1, §286, or §287, a function, or a lambda that returns a
   `Promise<void>`) takes §289. The call gives `Promise<void>`, and
   §289 rule 1a applies.
2. The returned handle completes after the handle of the callback
   completes, as for each adopted callback result (§186 rules 2 and
   3). If the handle of the callback fails, the returned handle fails
   with that exception. The order equals `node`.
3. Each other rule of §186 and §289 holds.
4. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 292.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a456-an-async-void-catch-callback-gives-a-promise-of-void` is
   rejected with the S013 of the table. The tracking note records the
   output.
2. `a456` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds the three rows of the
   table, a callback whose handle fails (caught by the caller), and an
   order test against a second promise chain, as `a454` does.
3. A unit test pins rule 3: `return bad().catch(async (e: Error) => {});`
   in a `Promise<void>` function gives one S013 of §289 rule 1a.
4. The full gate passes. `a456` has an async entry, so the LIR text
   snapshot grows by its functions. No other golden moves.

### 292.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. Five callback forms on
four receiver types ran as `await h.catch(r);` and passed `tsc`; 33
programs in other places each gave one S013 of §289 rule 1a. The order
of a callback with three awaits, a failing callback handle, and a
receiver that succeeds equals `node` on the three tiers. The live bytes
of 2, 50, and 100 iterations with `Context.collect()` did not grow, and
a firing control did.

1. MINOR, realistic, fixed: a test cost line lost its count (core
   principle 15).
2. MINOR, realistic, fixed: the S013 of §289 rule 1a for an async
   callback said "a `void` `catch` callback". The message names a
   callback that gives no value.
3. MINOR, contrived, fixed: a hot reload test ran two checker calls
   twice.
4. MINOR, realistic, fixed: the table of helper bodies had no row for
   an adopted `void` `catch` callback.

### 292.4 Open

None.
