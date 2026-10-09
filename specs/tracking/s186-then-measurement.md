# §186 — `then`, `catch`, and `finally` measurement

Date: 2026-10-09. Contract pin: `8c03b14c`. `node` v24.18.0, TypeScript
as pinned by the repository.

This round is a measurement round (CLAUDE.md, workflow step 0). Every
production change is reverted. The note records what the prototype
measured.

## Rule that the prototype contradicts

- `collisions.md` C8: "`Promise` construction and combinators stay
  rejected, except `Promise.all`".
- The §154 site `PromiseCombinatorCall` (S013, `Diverges(PromiseObject)`,
  witness `a-s047`) in `compiler/src/check/expr/call.rs`,
  `check_method_call`. The site rejects every call of a member named
  `then`, `catch`, or `finally`, before the checker reads the receiver
  type.
- Reject entry `r97-promise-combinator`.

## The prototype

The prototype changes the checker only. LIR, codegen, the runtime, and
the interpreter are not changed.

1. In `check_method_call` and in the `await` operand path
   (`compiler/src/check/expr/entry.rs`), a call of `then`, `catch`, or
   `finally` checks the receiver first. If the receiver type is
   `Promise<T>`, the call becomes a handle creation
   (`AsyncHandleCreate`) of an instance of a generic async helper. If
   the receiver is another type, the call is an ordinary method call.
2. The checker checks the first `then` callback with the parameter
   context `T` and no result context. Its result gives `U`. The checker
   then instantiates the helper with `<T, U>` and passes the checked
   receiver and callback as checked arguments.
3. The helpers are source functions that the test harness appends to
   each program. Under a contract, the compiler supplies them. The
   helper set selects by callback shape:

   | Form | Helper body |
   |---|---|
   | `h.then(cb)`, `cb: (v: T) => U` | `return cb(await h);` |
   | `h.then(cb)`, `cb: (v: T) => Promise<U>` | `const p = cb(await h); await done(); return await p;` |
   | `h.then(cb)`, `cb: (v: T) => void` | `cb(await h);` |
   | `h.then(cb)`, `cb: () => U` (the callback ignores the value, or `T` is `void`) | `await h; return cb();` |
   | `h.then(f, r)` | `let v: T; try { v = await h; } catch (e) { … return r(e); … } return f(v);` |
   | `h.catch(cb)`, `cb: (e: Error) => T` | `try { return await h; } catch (e) { … return cb(e); … }` |
   | `h.catch(cb)` on `Promise<void>` | the same, with no value |
   | `h.finally(cb)` | `try { return await h; } finally { cb(); await done(); await done(); }` |

   `done()` is an async function with an empty body. Each
   `await done()` is one turn of the ready queue (§94.1 rule 6). The
   padding makes the order equal to `node` (part 2). The first
   prototype had no padding; part 2 gives both results.
4. Captures (part 3): if a synchronous arrow callback captures a
   binding, the checker checks it again as an async arrow. The §181
   environment then owns its `const` captures, and the helper frame
   holds the callable. Its result becomes `Promise<U>`, so the adoption
   helper runs. The environment variable `S186_NO_ENV` turns this off
   for the control.

Three-tier runs use `check_program`, `lower_module` with `interpret`,
`run_jit`, and `run_c_aot` in one temporary test file. `node` runs use
`corpus/node/run-js-corpus.cjs`. `tsc` runs use the repository
`tsconfig.json` options with `prelude/lang.d.ts`.

## 1. Semantics in the three tiers

`tsc` accepts each program. "Match" means interpreter, dev JIT, and
C AOT print the `node` output, byte for byte.

| Program | Content | Result |
|---|---|---|
| chain | `value(1).then(a).then(b)`; a held handle and `then` with an unannotated parameter | match |
| adopt | a callback that returns `value(v + 100)`; `then(v => value(v * 3)).then(v => v + 1)` | match |
| throw | a named callback throws; the next `then` does not run; `try` around `await` catches | match |
| catch | `catch` recovers a rejection; `catch` on a fulfilled handle passes the value; a rejection passes through `then` to `catch`; `then(f, r)` | match |
| finally | `finally` on a fulfilled and a rejected handle; a `finally` callback that throws replaces the result | match |
| counted results | the callback returns a `Generator<i32>`, a `Promise<i32>[]`, an `i32[]` | match |
| void | `then` with a `void` callback; `then` on `Promise<void>`; `finally` and `catch` on `Promise<void>`; a zero-parameter callback on `Promise<i32>` | match |
| holders | a `then` handle in a field, in an array, in `Promise.all`; a function value as the callback (sync and async) | match |
| rethrow | a `catch` callback that throws; `catch(…).finally(…)` | match |
| fan-out | two `then` calls on one handle, then `await` of all three; `then` on an array element | match |
| sync producer | a synchronous function returns `h.then(cb)`; the caller holds and awaits it | match |

Programs: appendix A.

## 2. Order against `node`

### 2.1 The base model

`Context.suspend()` is one microtask under the `node` shim
(`Promise.resolve()`). In this language it parks the frame until the
next checkpoint, and every ready chain drains in the current checkpoint
(§94.1 rules 3 and 8). A program that mixes `Context.suspend()` with
handle awaits therefore differs from `node` without `then`. Control at
the pin, no `then`:

```ts
async function now(n: i32): Promise<i32> { return n; }
async function ticker(): Promise<void> {
  for (let i: i32 = 1; i <= 4; i++) { print(`t${i}`); await Context.suspend(); }
}
export async function main(): Promise<void> {
  const t = ticker();
  const w = (async (): Promise<void> => { print(`await ${await now(2)}`); })();
  print("sync");
  await w; print("main after w");
  await t;
}
```

`node`: `t1 sync t2 await 2 t3 main after w t4`. The three tiers:
`t1 sync await 2 t2 main after w t3 t4`. This difference is in the
base model and not in `then`.

The order programs below use a ticker that awaits a completed handle
(`await now(0)`). That is one ready-queue turn in this language and one
microtask in `node`. With that ticker the control prints the `node`
output in the three tiers.

### 2.2 Ticks per form

Program `ticks` (appendix B): a ticker root increments a counter each
turn; `measure` records the turns from the creation of a handle to the
resume of its `await`.

| Form | `node` | padded helpers | unpadded helpers |
|---|---|---|---|
| `await` of a completed handle | 1 | 1 | 1 |
| `now(1)` | 1 | 1 | 1 |
| `now(1).then(cb)` | 2 | 2 | 2 |
| `then` whose callback returns a completed handle | 4 | 4 | 3 |
| `then(cb).then(cb)` | 3 | 3 | 3 |
| `then` on a rejected handle | 2 | 2 | 2 |
| `then(f, r)` on a rejected handle | 2 | 2 | 2 |
| `catch` on a rejected handle | 2 | 2 | 2 |
| `catch` on a fulfilled handle | 2 | 2 | 2 |
| `finally` on a fulfilled handle | 4 | 4 | 2 |
| `finally` on a rejected handle | 4 | 4 | 2 |
| `Promise.all([…]).then(cb)` | 3 | 3 | 3 |
| adoption of a handle that completes 3 turns later | 6 | 6 | — |

The unpadded rows are the plain async-function model. `node` with
`Promise.prototype.then`, `catch`, and `finally` replaced by the same
unpadded helpers prints the unpadded column, so the difference is in
the JavaScript algorithms, not in this language:

- Adoption: `node` resolves the derived promise with a thenable. That
  takes `NewPromiseResolveThenableJob` and one reaction: 2 turns.
  `return await p` takes 1 turn. One `await done()` before
  `await p` closes the difference.
- `finally`: `node` returns `PromiseResolve(onFinally()).then(() => value)`
  from the reaction, which is a thenable resolution: 2 more turns. Two
  `await done()` close it.

### 2.3 Interleaved programs

Five programs interleave `then` callbacks, `await` continuations, and
`Promise.all` (appendix B): a `then` on a completed handle, on a
pending handle, an adoption chain beside a plain chain, a mixed program
with an awaiter, `Promise.all`, and a two-step `then` chain, and a
`catch`/`finally` program. With the padded helpers, the three tiers
print the `node` output byte for byte for all five. With the unpadded
helpers, the adoption program and the `catch`/`finally` program differ
by the turns of 2.2.

## 3. Captures

### 3.1 Results

| Program | `node` | Prototype, three tiers |
|---|---|---|
| a `then` callback captures `const` `i32`, `string`, and a reference object in a function that returns the handle; three chains; `Context.collect()` while they are pending; 100 same-size allocations; then `await` | `L1 11 1`, `L2 22 2`, `L3 33 3` | match |
| a callback captures a `Generator<i32>` and a `Promise<i32>[]`; `Context.collect()` while pending | `9`, `1 2 8` | match |
| a function captures a generator, returns the `then` handle, and its binding exits; `Context.collect()`; 100 allocations | `7` | match |
| `catch` and `finally` callbacks capture `const` bindings | `done`, `42` | match |
| control, `S186_NO_ENV`: the same capturing callbacks with no environment | — | S009 "lambda capturing `label`, `base`, `box` may capture at held async argument" (§118) |
| a callback captures a handle and returns it (adoption) | `5` | S009 (the prototype keeps a callback that returns a handle synchronous) |
| a callback captures a `let` that changes before the callback runs | `11` | S009 "async arrow captures mutable `n`; copy it into a `const` first, or use a class with a field" |

### 3.2 Collection and counts

Dev JIT, `ReloadSession`, 1,000 sequential chains `await now(i).then(cb)`:

| Callback | Live allocations after the run | After `Context.collect()` |
|---|---|---|
| non-capturing | 1 | 0 |
| captures `const k` | 1,001 | 0 |

Each capturing chain leaves one environment until an explicit
collection (invariant 2). Collection reclaims each one.

### 3.3 Hot reload

Dev JIT. `main` creates `later(1).then(cb)`, where `later` awaits
`Context.suspend()`. The host kicks `main`, reloads with a changed
callback body, and steps.

| Callback | No reload | Reload |
|---|---|---|
| captures `const k` | `2` | `StaleCoroutine` at the `await Context.suspend()` of `later` |
| non-capturing arrow | `2` | `StaleCoroutine`, same position |
| named function | `2` | `StaleCoroutine`, same position |

The helper frame is an ordinary async frame, so it follows the
stale-frame rule (§167 rule 10, §181 rule 7). No new reload rule is
necessary.

### 3.4 The cost of the async conversion

The conversion makes the callback an async callable. Its result is a
completed handle, and an `await` of a completed handle is one turn
(§94.1 rule 2). Program `f-capture-ticks` (appendix C):

| Form | `node` | Prototype |
|---|---|---|
| non-capturing `then` | 2 | 2 |
| capturing `then` | 2 | 4 (adoption helper: `await done()` and `await p`) |
| capturing `finally` | 4 | 3 (`await cb()`, no padding) |

A capturing callback cannot match `node` order through an async
conversion: at least one turn is added. A synchronous callable that
owns a §181 environment has the `node` order. The existing lowering
gives the environment only to an async lambda:

- `codegen/src/lir/lambda.rs`, `lower_lambda_with_id`: the environment
  class is allocated only if `is_async && !captures.is_empty()`.
- `codegen/src/lir/builder.rs`: the `OwnedEnvironment` parameter and
  the field loads exist only on the async branch.
- `codegen/src/lower/func/builtin.rs`, `make_closure`: an
  `OwnedEnvironment` callable becomes the pair (`LirWrapper` code,
  environment object). A synchronous capturing callable stores its
  captures in a stack slot of the uniform closure layout, and the pair
  points at that slot.
- The C emission (`codegen/src/cemit/graph.rs`, `emit_closure`) and the
  interpreter (`codegen/src/interpreter/instruction.rs`, `MakeClosure`)
  have the same split.

A synchronous owned environment needs a synchronous wrapper with an
`OwnedEnvironment` parameter in the three tiers. The function-value
root of §181 rule 4 already roots the environment word.

### 3.5 A `let` capture

`node` prints `11`: the callback reads the binding when it runs. A copy
at creation prints `2`. A `let` capture needs the cell of the §181
measurement (`specs/tracking/s181-async-closure-measurement.md`,
part B3): one shared cell for each binding, every read and write of the
binding through the cell, and the cell rooted by the environment.

## 4. Contextual typing

| Form | `tsc` | Prototype |
|---|---|---|
| `then(v => v + 1)` | accepts | accepts: `v: i32` from `T`, `U = i32` from the body |
| `then(v => \`s${v}\`)` | accepts | accepts, `U = string` |
| `then((v: i32) => v + 1)` | accepts | accepts |
| `then(() => 9)` | accepts | accepts (zero-parameter helper) |
| `then(v => { return v + 1; })` | accepts | S100, block body with no result annotation (C24 row 4) |
| `then((v: string): string => v)` on `Promise<i32>` | TS2345 | S100, argument type mismatch |
| `then(v => print(…))` (`r97`) | accepts | S100 "a `void` expression is only allowed as an expression statement" |

The parameter comes from the receiver, and the result comes from the
body, as for `Array.prototype.map`. The same block-body rejection and
the same `void` expression rejection apply at the pin to
`xs.map(v => { return v + 1; })` and to `xs.forEach(v => print(…))`.

A generic call with an unannotated callback parameter is rejected at
the pin (S100, "Generic argument inference checks callback parameters
before the callee supplies their context"). A hand-written helper call
`thenOf(value(1), v => v + 1)` therefore needs `<i32, i32>`. The
method form does not, because the receiver gives `T`.

`T` is not inferred from a `Promise<T>` argument at the pin:
`finallyOf(value(3), cb)` with `finallyOf<T>(h: Promise<T>, cb: () => void)`
is S100 "cannot infer type parameter `T`: no candidate".

## 5. `catch` typing

TypeScript types `catch` as `Promise<T | U>` and the callback parameter
as `any`. The prototype checks the callback with the parameter `Error`
and the result context `T`.

| Form, on `Promise<i32>` | `tsc` | Prototype |
|---|---|---|
| `catch(e => 0)` | accepts | accepts |
| `catch(e => e.message.length)` | accepts | accepts |
| `catch((e: Error): i32 => { throw e; })` | accepts | accepts |
| `catch(e => "x")` | accepts, `string \| number` | S100 "the lambda body expects `i32`, got `string`" |
| `const r: i32 = await …catch(e => "x")` | TS2322 | S100, same |
| `catch((e: Error): void => { print(e.message); })` | accepts, `number \| void` | S100 "the argument expects `(Error) => i32`, got `(Error) => void`" |
| `catch(e => { print(e.message); })` | accepts | S100 "not all paths return a value" |
| `catch((e: unknown): i32 => 0)` | accepts | S100, keyword type outside the surface |
| `catch((e: Error): i32 \| null => null)` | accepts | S011, `i32 \| null` |
| `then((v: i32): i32 => v, (e: Error): string => e.message)` | accepts | S100, second argument type |
| `finally(() => 5)` | accepts | S100 "the lambda body expects `void`, got `i32`" |
| `finally((x: i32): void => {})` | TS2345 | S100 |

The restriction to `T` needs no new checker mechanism: the result
context `T` gives each rejection above. The common logging form, a
`void` callback on a non-`void` handle, is rejected by it. The value
of that form is used in a statement position in each measured case.

## 6. A dropped handle

S013 is unchanged in the prototype: `h.then(cb);` with no holder is
S013 "an async handle is dropped without any await of its completion".
A field holder whose second assignment replaces a pending handle is
also S013.

`Context.free` of a holder drops a pending handle at run time. The pin
runtime already has the behaviour of §178 rule 8 for script frames:

| Program | Three tiers |
|---|---|
| a holder of a pending `then` handle is freed; the callback prints | `freed`, `got 1`, `main end` |
| the same, and the callback throws | `freed`, `boom runs`, then trap 29 `UncaughtException` at the `throw` in the callback |
| control without `then`: a freed holder of a pending frame | `freed`, `later 1`, `main end` |
| control: the frame throws after the free | trap 29 at the `throw` in the frame |

- The count: the scheduler registration of each frame (§94.2) is the
  producer count. No new count is necessary.
- The release point: the completion of the frame.
- The trap position: the last `throw` (§116.1 rule 4). §178 rule 8 uses
  the creation position because a host source has no `throw`.
- Memory of a chain that never completes, dev JIT, 100 dropped chains
  on a frame that never completes, after `Context.collect()`:

  | Chain | Live allocations | `async_unfinished` |
  |---|---|---|
  | none | 1 | 0 |
  | frame only | 101 | 100 |
  | `.then(cb)` | 201 | 200 |
  | `.then(cb)` with a `const` capture | 301 | 200 |
  | `.then(cb).then(cb)` | 301 | 300 |

  Each `then` holds one frame, and a capturing callback adds one
  environment. Collection does not reclaim them, because the
  registrations root them. They stay until Context release.

`node`, measured: a dropped `then` whose callback runs prints its
output after `main` and exits 0. A dropped `then` whose callback throws
prints the error at the `throw`, exits 1, and a later timer does not
run.

## 7. A receiver that is not a handle

```ts
class Box {
  n: i32 = 7;
  then(cb: (v: i32) => void): void { cb(this.n); }
}
export async function main(): Promise<void> {
  const b = new Box();
  b.then((v: i32): void => { print(`direct ${v}`); });
  const v = await b;
  print(`await ${v}`);
}
```

- `tsc` accepts. `node` prints `direct 7`, `await 7`: `await` calls
  `then(resolve, reject)` of a thenable and adopts its value.
- At the pin: `b.then(…)` is S013 "Promise combinator `.then(...)` is
  not in the language", although `Box` declares `then`. A class
  `catch` method gets the same S013. `await b` is S100.
- In the prototype, the receiver type selects the path. `b.then(…)` and
  `b.catch(…)` are ordinary method calls and print the `node` output in
  the three tiers. `await b` stays S100: `await` reads a handle, and a
  class has no handle type (C1 nominal types).

## 8. Cost

Release CLI with the prototype, best of three, C AOT executables and
`subscript run` (dev JIT). The net time subtracts a run with zero
chains.

| Form | C AOT, 10,000 chains | C AOT, 1,000,000 chains | dev JIT, 1,000,000 chains |
|---|---|---|---|
| `await now(i).then(add1)` | 212.8 ns | 179.3 ns | 300.4 ns |
| hand-written `await myThen(now(i), add1)` | 199.5 ns | 180.3 ns | 293.6 ns |
| no callback, `add1(await now(i))` | 78.2 ns | 66.9 ns | 116.6 ns |
| capturing `then((v) => v + k)` (async conversion) | 392.0 ns | 402.8 ns | 711.5 ns |
| hand-written class with a field and an async method | 244.0 ns | 211.8 ns | 399.5 ns |

- The synthesized form costs the same as the hand-written wrapper
  (0.99 at 1,000,000 chains, C AOT). The 10,000-chain totals are 2 ms
  and are inside the noise of process start.
- A `then` costs one frame more than an inline callback: 2.7 times in
  C AOT.
- The async conversion of a capturing callback costs 1.90 times the
  hand-written class (C AOT, 1,000,000 chains).

Checker, release, best of three: a program of 1,000 `.then` calls
checks in 36.1 ms, and the same program with 1,000 direct helper calls
in 35.4 ms. `subscript check` over the 315 `corpus/accept/*.ts` files:
1.21 s at the pin binary and 1.21 s with the prototype (second run of
each).

## Decisions a contract needs

1. **Admission.** Accept `then`, `catch`, and `finally` on a
   `Promise<T>` receiver as a handle creation (C8, `PromiseCombinatorCall`).
   Measured: the checker rewrite needs no LIR, codegen, or runtime
   change, and matches `node` in the three tiers for every program of
   part 1.
2. **Receiver dispatch.** Options: (a) dispatch on the receiver type,
   so a class `then` method is an ordinary call; (b) keep the
   name-only rejection. Measured: (a) fixes the class-method rejection
   of part 7. `await` of a thenable stays rejected in both.
3. **Order.** Options: (a) padded helpers, equal to `node` for every
   measured form (part 2); (b) the plain async-function model, 1 turn
   earlier for adoption and 2 turns earlier for `finally`. (a) costs
   one more `await` for each adoption and two for each `finally`.
4. **Where the helpers live.** The prototype appends source helpers.
   Options: compiler-supplied generic instances (as the prototype), or
   one runtime task kind with a callback. Measured only for the first.
5. **Helper selection.** The callback shape selects among eight helper
   bodies (`void` result, `void` value, zero parameters, adoption,
   `then(f, r)`). A contract states each shape and its result type.
6. **Captures.** Options: (a) a synchronous callable that owns a §181
   environment, rooted by the helper frame, with the `node` order;
   (b) an async conversion, measured to work in the three tiers, but at
   least 1 turn later than `node` and 1.90 times the cost of the class
   form; (c) keep S009 (§118 escape). A `let` capture needs cells in
   every option.
7. **A capturing callback that returns a handle** (adoption with a
   capture). The prototype keeps it S009. It needs option 6a.
8. **`catch` result type.** Options: (a) restrict to `T`, as measured;
   (b) also accept a `void` callback, with a `Promise<void>` result,
   for the logging form; (c) `T | U` is not in the language (S011 for
   scalars). (b) has no `tsc` divergence in statement position.
9. **Contextual typing.** The parameter from `T` and the result from
   the body work with no new rule. The block-body and `void`-expression
   rejections are the existing callback rules; `r97` hits the second.
10. **A dropped `then` handle.** Options: (a) keep S013; (b) accept a
    dropped `then`/`catch`/`finally` handle as a producer-held task,
    with trap 29 at the last `throw` if it fails, as the runtime does
    today for a freed holder. (b) holds one frame per link until the
    chain completes, and forever for a chain that never completes.
11. **Hot reload.** No new rule: a suspended helper frame follows the
    stale-frame rule.
12. **Corpus moves.** `r97-promise-combinator` and the witness `a-s047`
    are dropped handles in a synchronous `main`. Under the prototype
    both stay S013 at the dropped handle; `r97` also gets the `void`
    expression S100. No accept, warn, or trap entry calls `then`,
    `catch`, or `finally`.

## Appendix A — semantic programs

`value(n)` is `async function value(n: i32): Promise<i32> { await Context.suspend(); return n; }`
and `fail(n)` throws `new Error(\`fail ${n}\`)` after the same `await`.

```ts
// chain
const r = await value(1).then((v: i32): i32 => v + 1).then((v: i32): string => `s${v}`);
print(r);                                   // s2
const h = value(10);
const h2 = h.then(v => v * 2);
print(`${await h2}`);                       // 20

// adopt
print(`${await value(1).then((v: i32): Promise<i32> => value(v + 100))}`);   // 101
print(`${await value(2).then(v => value(v * 3)).then(v => v + 1)}`);         // 7

// throw: boom(v) throws new Error(`cb ${v}`)
try { await value(1).then(boom).then((v: i32): i32 => v + 1); }
catch (e) { if (e instanceof Error) print(`caught ${e.message}`); }          // caught cb 1

// catch
print(`${await fail(1).catch((e: Error): i32 => e.message.length)}`);       // 6
print(`${await value(5).catch(e => 0)}`);                                    // 5
print(`${await fail(2).then((v: i32): i32 => v + 1).catch(e => -1)}`);       // -1
print(await fail(3).then((v: i32): string => `ok ${v}`, (e: Error): string => `rej ${e.message}`)); // rej fail 3

// finally
print(`${await value(4).finally((): void => { print("fin a"); })}`);        // fin a, 4
try { await fail(5).finally((): void => { print("fin b"); }); }
catch (e) { if (e instanceof Error) print(`caught ${e.message}`); }          // fin b, caught fail 5
try { await value(6).finally((): void => { throw new Error("from finally"); }); }
catch (e) { if (e instanceof Error) print(`caught ${e.message}`); }          // caught from finally

// counted results
const g = await value(3).then((v: i32): Generator<i32> => gen(v));          // 3 4
const hs = await value(2).then((v: i32): Promise<i32>[] => [value(v), value(v + 1)]); // 2 3

// holders: double(v) returns v * 2
const o = new Holder(value(21).then(double));                                // 42
const all = await Promise.all([value(3).then(double), value(4).then(double)]); // 6,8

// fan-out
const h1 = value(1);
const a = h1.then((v: i32): i32 => v + 10);
const b = h1.then((v: i32): i32 => v + 20);
print(`${await h1} ${await a} ${await b}`);                                  // 1 11 21
```

## Appendix B — order programs

```ts
// ticks
let tick: i32 = 0;
async function now(n: i32): Promise<i32> { return n; }
async function fail(): Promise<i32> { throw new Error("x"); }
async function ticker(): Promise<void> {
  for (let i: i32 = 0; i < 200; i++) { tick = tick + 1; await now(0); }
}
async function measure(name: string, h: Promise<i32>): Promise<void> {
  const t0 = tick;
  try { await h; } catch (e) { }
  print(`${name} ${tick - t0}`);
}
export async function main(): Promise<void> {
  const t = ticker();
  await measure("then", now(1).then((v: i32): i32 => v));
  await measure("then-adopt", now(1).then((v: i32): Promise<i32> => now(v)));
  await measure("finally-fulfilled", now(1).finally((): void => { }));
  // … one line for each row of the table in 2.2
  await t;
}
```

```ts
// mixed (later(n) awaits now(0) once, then returns n)
export async function main(): Promise<void> {
  const t = ticker();                       // prints t1..t8, one turn each
  const a = awaiter();                      // awaits later(10), then now(11)
  const all = Promise.all([later(1), now(2)]);
  const th = later(3).then((v: i32): i32 => { print(`then ${v}`); return v; })
                     .then((v: i32): void => { print(`then2 ${v}`); });
  const ah = (async (): Promise<void> => { const xs = await all; print(`all ${xs.join(",")}`); })();
  print("sync");
  await th; print("main th");
  await ah; await a; await t;
}
// node and the three tiers: t1 sync t2 t3 awaiter 10 then 3 t4 awaiter 11
// all 1,2 then2 3 t5 main th t6 t7 t8
```

## Appendix C — capture programs

```ts
// const captures, collection while pending
class Box { n: i32 = 0; constructor(n: i32) { this.n = n; } }
function start(k: i32): Promise<string> {
  const base: i32 = k * 10;
  const label: string = `L${k}`;
  const box = new Box(k);
  return later(k).then((v: i32): string => `${label} ${v + base} ${box.n}`);
}
export async function main(): Promise<void> {
  const hs: Promise<string>[] = [];
  for (let i: i32 = 1; i <= 3; i++) { hs.push(start(i)); }
  Context.collect();
  const junk: Box[] = [];
  for (let i: i32 = 0; i < 100; i++) { junk.push(new Box(-1)); }
  for (const h of hs) { print(await h); }
}
```

```ts
// f-capture-ticks (ticker and now as in appendix B)
const k: i32 = 5;
let t0 = tick;
await now(1).then((v: i32): i32 => v + k);
print(`capturing ${tick - t0}`);            // node 2, prototype 4
```
