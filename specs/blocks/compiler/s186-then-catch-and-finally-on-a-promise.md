<!-- §186 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 186. `then`, `catch`, and `finally` on a promise

*(Added 2026-10-09.)* Origin: the owner asked on 2026-10-09 what
`Promise.prototype.then` needs, judged by technical feasibility only.
On 2026-10-09 the owner selected this section and decided: the order
of callbacks matches `node`; a synchronous callback can own its
`const` captures; a `catch` callback returns `T` (a `void` callback
only when `T` is `void`); S013 stays (every handle has an awaited
completion). The measurement round at `8c03b14c` is
`specs/tracking/s186-then-measurement.md`.

Problem: `h.then(cb)`, `h.catch(cb)`, and `h.finally(cb)` are S013
(collision C8, site `PromiseCombinatorCall`), although each one has the
meaning of a call of a small async function that the language already
accepts. The measurement rewrote each call into such a function in the
checker, with no LIR, codegen, or runtime change. Every program of the
measurement printed the `node` output in the interpreter, the dev JIT,
and C AOT, at 0.99 times the cost of a hand-written async wrapper
(C AOT, 1,000,000 chains). The site also rejects a class's own `then`
method by its name.

### 186.1 Rules

1. **Forms.** On a receiver whose apparent type is `Promise<T>`:
   - `h.then(f)`: `f: (v: T) => U` gives `Promise<U>`; `f: (v: T) =>
     Promise<U>` gives `Promise<U>` (adoption).
   - `h.then(f, r)`: `r: (e: Error) => U` (or `Promise<U>`), with the
     same `U` as `f`.
   - `h.catch(r)`: `r: (e: Error) => T` (or `Promise<T>`) gives
     `Promise<T>`. A `void` callback is accepted only when `T` is
     `void`.
   - `h.finally(f)`: `f: () => void` gives `Promise<T>`.

   A callback on a `void` value (`Promise<void>`) has no parameter.
   The language has no `void` parameter type, so a parameter on that
   callback is rejected (rule 6). Each call is an async
   origin (§178 rule 1): S013 applies to its handle, as to any handle.
2. **Meaning.** Each form has the meaning of a call of a generic async
   function that the compiler supplies: `then` awaits `h` and returns
   the callback result (awaited for adoption); `then(f, r)` and `catch`
   catch the exception of `await h` and call `r`; `finally` awaits `h`
   in a `try` whose `finally` block calls `f` (§180). An exception that
   a callback throws completes the returned handle.
3. **Order.** The number of turns from the completion of `h` to the
   callback, and from the callback to the completion of the returned
   handle, equals `node`'s: the measurement found that a plain async
   function is 1 turn early for adoption and 2 turns early for
   `finally`, and that one more `await` for adoption and two for
   `finally` give the `node` order for every measured program.
4. **Callbacks.** A callback is a function value, a named function, or
   a lambda. Its parameter type comes from `T` (or `Error`) and its
   result type from its body, as for an array `map` callback. A
   literal result of a rejection or `catch` callback with no result
   annotation takes its type from `U` or `T` (`catch((e: Error) => 0)`
   on `Promise<i64>` gives `i64`). A
   synchronous lambda that is a direct callback argument of these forms
   can capture a `const` local: an environment object owns the captures
   (§181 rules 2–5 apply), and the synchronous callable holds the
   environment. A capture of a `let`, `var`, or `this` stays S009.
5. **Receiver.** The forms apply only to a receiver whose apparent type
   is `Promise<T>`. A method named `then`, `catch`, or `finally` of
   another type is an ordinary method call. `await` of a value that is
   not a handle stays rejected (a thenable is not adopted; C8).
6. **Rejected with a divergence.** These forms are rejected with a
   divergence block that shows the accepted form, because `tsc`
   accepts them:
   - a `catch` callback whose result is not `T`, and a `void` callback
     on a non-`void` handle (`tsc` gives `Promise<T | U>`);
   - a callback parameter on a `Promise<void>` value;
   - a callback parameter whose type is not the delivered type (`T`,
     or `Error` for a rejection callback) but that `tsc` accepts, for
     example `T | null`. Function types have no variance (C24 row 25).

   A form that `tsc` rejects is reported at a site that records the
   `tsc` result, measured.
7. **Tiers.** The interpreter, the dev JIT, and C AOT give the same
   output, order, and traps.

8. **Hot reload.** The helpers are ordinary functions, not §119
   helpers, because each one takes a callback parameter (§119.1
   rule 4). The checker adds a helper at the first use of its key, so
   it is part of the declaration hash (§8.2). A body edit that adds a
   new key, or that changes the order of two calls with different
   keys, is a refused swap (`DeclarationChanged`), as for a new
   generic instance.

### 186.2 Acceptance

1. Red first, at the contract pin: accept entries for chains,
   adoption, a callback exception, `catch` recovery, `then(f, r)`,
   `finally` with a rethrow, a capturing synchronous callback with a
   `Context.collect()` while the chain is pending, a class with its own
   `then` method, and an order program that mixes `then`, `await`, and
   `Promise.all`; each `js-comparable: yes` where `node` prints the
   same bytes. Reject entries for a dropped `then` handle (S013), a
   `let` capture (S009), and a `catch` callback of another type.
2. Unit tests: the rooting of a callback environment in each tier
   (core principle 9: a built environment and a same-shape control
   without one); the turn counts of rule 3 against `node`.
3. Cost: record the chain cost against the hand-written wrapper and
   the checker cost.
4. Goldens: the LIR text golden for the new entries; the generated
   docs; the TypeScript tutorial async section. `r97` moves if its
   message changes.

### 186.3 Sections this one amends

- Collision C8: `then`, `catch`, and `finally` on a handle are
  accepted; other combinators and thenable adoption stay rejected.
- §175 and §181: a synchronous lambda that is a direct callback of
  these forms owns its `const` captures.
- §182.4 item 3: a class's own `then` method is an ordinary call.

### 186.4 Open

1. The parameter-mismatch rule calls `ts_nominal_assignable`, which
   approximates `tsc`. Two arms give a TscRejects site for a form that
   `tsc` accepts: a function-typed parameter with a different
   parameter count (`rejection_facts.rs`, `Func` arm), and two generic
   classes with the same structure (`instance_arguments`). The other
   assignment sites share both arms. The rejection is sound; only the
   `tsc` claim is wrong.
2. `then(f, null)` gives a message and an example that show
   `catch(r)`; the accepted form is `then(f)`.
3. The `PromiseReactionSpread` divergence block shows the §14.4
   spread example, not a reaction form.
4. A field comment at `compiler/src/check/mod.rs` cites rule 6 for the
   literal hint; rule 4 holds it.
5. A generic callback result `T` instantiated as `Promise<i32>` is
   adopted by the helper, so `return await p.then((v: i32): T => x)`
   gives S100 with no divergence block.
6. A handle inside an awaited value (`(await many()).length`, with
   `many(): Promise<Promise<i32>[]>`) is dropped with no S013. This
   gap is older than this section.
