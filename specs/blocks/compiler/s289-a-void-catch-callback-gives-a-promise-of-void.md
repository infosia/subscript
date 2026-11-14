<!-- §289 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 289. A `void` `catch` callback gives a `Promise<void>`

*(Added 2026-11-14.)* Origin: §238.4 item 4. The owner selected the item
on 2026-11-14. It changes the owner decision of 2026-10-09 in §186: "a
`catch` callback returns `T` (a `void` callback only when `T` is
`void`)".

Problem, measured at `c35993fd` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): void` is a function, `wi(): Promise<i32>`
and `bad(): Promise<i32>` are async, and `bad` throws.

| Form | subscript | `tsc` |
|---|---|---|
| `await wi().catch(() => v());` | S013 "a `catch` callback must return `i32`; a `void` callback is accepted only on `Promise<void>`" | accepts, `Promise<number \| void>` |
| `await bad().catch((e: Error) => { print(e.message); });` | S013, the same | accepts |
| `const r = await wi().catch(() => v()); if (r !== undefined) print(`${r + 1}`);` | S013, the same | accepts |

A `void` `catch` callback handles a failure and gives no value. The
program reads no value of the result, or reads it only after a test for
`undefined`. `tsc` gives the result `Promise<T | void>`. The language has
no union with `void`, so §186 rejects the form.

### 289.1 Rules

1. `h.catch(r)` on a receiver `Promise<T>` with `T` other than `void`,
   and a callback `r` with the result `void` (§238 rule 1, §286, or a
   `void` function), gives `Promise<void>`. If `h` completes with a
   value, the returned handle completes with no value, and the value is
   dropped. If `h` fails, `r` runs, and the returned handle completes
   with no value, or fails with the exception that `r` throws.
1a. The call is accepted only as the operand of an `await` that is an
   expression statement: `await h.catch(r);`. Each other place of the
   call gives S013 with a message that names the `tsc` type
   `Promise<T | void>`. `tsc` rejects the call in a place of type
   `Promise<void>`, such as a `return` of a `Promise<void>` function, a
   parameter, an annotated binding, or an array element, because
   `Promise<T | void>` does not convert to `Promise<void>`. The language
   has no type for `Promise<T | void>`, so the checker cannot tell those
   places from the places that `tsc` accepts. *(Amended 2026-11-14 after
   the Phase Review, finding 1.)*
2. The order of callbacks is the order of §186 rule 3 for `catch`,
   which equals `node`.
3. The value of the result is `void`. So a read of the awaited value,
   as in the third row of the table, gets the diagnostic that a `void`
   value gets. *(`tsc` accepts a read after a test for `undefined`. It
   is recorded in 289.4.)*
4. Each other rule of §186 holds. A `catch` callback with a result that
   is not `T` and not `void` keeps its S013. A `void` rejection callback
   of `then(f, r)` keeps its S013.
5. A form that the checker accepts at the pin keeps its type and its
   output.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 289.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a454-a-void-catch-callback-gives-a-promise-of-void` is rejected with
   the S013 of the table. The tracking note records the output.
2. `a454` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds the first two rows of the
   table, a callback that throws (caught by the caller), and an
   order test against a second promise chain, as the §186 entries do.
3. A unit test pins rules 1a, 3, and 4: the third row of the table
   gives one diagnostic; `return wi().catch(v);` in a `Promise<void>`
   function, `const h = wi().catch(v);`, and an element of a
   `Promise<void>[]` each give one S013 of rule 1a; a `catch` callback that returns `string` on
   `Promise<i32>` keeps its S013, and a `void` rejection callback of
   `then(f, r)` keeps its S013.
4. The full gate passes. `a454` has an async entry, so the LIR text
   snapshot grows by its functions. No other golden moves.

### 289.3 Phase Review

Pending.

### 289.4 Open

1. Rule 3: a read of the awaited value after a test for `undefined`
   stays rejected. `tsc` accepts it.
2. Rule 1a: a handle of the call kept in a `const` with no annotation
   and then awaited, an element of `Promise.all`, and a receiver of a
   further `then`, `catch`, or `finally` stay rejected. `tsc` accepts
   them.
3. An async `void` `catch` callback on `Promise<T>` with `T` other than
   `void` stays rejected. `tsc` accepts it.
