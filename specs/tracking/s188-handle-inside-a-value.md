# §188 measurement — a handle inside a value

Date: 2026-10-10. Measured at `85c2fbcf`. `node` v24.18.0. TypeScript
5.9.2 with `prelude/lang.d.ts` and the repository compiler options.

This round is a measurement round (CLAUDE.md, workflow step 0). Every
production change is reverted. The note records what the prototype
measured. Origin: `compiler.md` §186.4 item 6.

## Rule that the prototype contradicts

- §70.1 decision 2: "Dropping it without awaiting remains rejected."
  §70.4 item 4 (`r157`), and the rewritten `r100` and `r105`.
- §186, owner decision of 2026-10-09: "S013 stays (every handle has an
  awaited completion)". Reject entry `r402`.
- The §154 site `AsyncHandleUnawaited` (S013, `Diverges(DroppedAsyncHandle)`).

Option A extends these rules. Options B and C remove part of them.

## Method

Each program of part 1 has two variants. `WORK` is `work(1)` in the
completed variant and `fail()` in the failed variant. Every program
starts with:

```ts
async function work(v: i32): Promise<i32> { return v; }
async function fail(): Promise<i32> { throw new Error("lost"); }
```

- `subscript check`: the release CLI built from the pin.
- Three tiers: `check_program`, `lower_module` with `interpret`,
  `run_jit`, and `run_c_aot` in one temporary test file. "Retained"
  is the length of `ReloadSession::async_tasks` after `main` and every
  ready step.
- `node`: one process per program. TypeScript `transpileModule`, the
  corpus shim (`corpus/node/js-corpus-shim.cjs`), then `main()`. The
  exit code is 1 on an unhandled rejection.
- `tsc`: one `tsc -p` run over every program of this note. It accepts
  every program except `g14` (TS2339, part 4).

"T29" is trap 29, `UncaughtException`, `Error: lost`, at the `throw`
in `fail` (2:39), in the interpreter, the dev JIT, and C AOT. The three
tiers agree on every row below; one result stands for the three.

## 1. The forms

### 1.1 Forms that the pin accepts

| Id | Form (`main` body or helper) | Completed: output, retained | Failed | `node` completed / failed |
|---|---|---|---|---|
| f01 | `many(): Promise<Promise<i32>[]>`; `print(\`${(await many()).length}\`)` | `2 end`, 0 | T29, stdout empty | `2 end` / `2 end`, exit 1 |
| f02 | `nested(): Promise<Promise<i32>>`; `await nested();` | `end`, 0 | T29, stdout empty | not comparable: `node` flattens; the failed variant rejects `main` |
| f03 | `const inner = await nested();`, never read | `end`, 0 | T29 after `end` | not comparable, as f02 |
| f05 | `grid(): Promise<i32>[][]` (synchronous); `grid().length` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| f06 | `many().then((xs: Promise<i32>[]): i32 => xs.length)`, awaited | `2 end`, 0 | T29, stdout empty, retained 2 | `2 end` / `2 end`, exit 1 |
| f07 | `nested().then((p: Promise<i32>): i32 => 5)`, awaited | `5 end`, 0 | T29, stdout empty | not comparable, as f02 |
| f08 | `const f = (p: Promise<i32>): i32 => 1; f(WORK)` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| f10 | `const [a, b] = await many(); await a;` (`b` is `WORK`) | `1 end`, 0 | T29 after `1 end` | `1 end` / `1 end`, exit 1 |
| f12 | `for (const h of await many()) { n++; }` | `2 end`, 0 | T29, stdout empty | `2 end` / `2 end`, exit 1 |
| f16 | `m.set(1, WORK)` on a local `Map<i32, Promise<i32>>` | `1 end`, 1 | no trap, `1 end`, retained 1 | `1 end` / `1 end`, exit 1 |
| f22 | `G.push(WORK); G = [];` (module global) | `end`, 0 | T29, stdout empty | `end` / `end`, exit 1 |
| f23 | `await (await many())[0]` (element 1 is `WORK`) | `1 end`, 0 | T29, stdout empty, retained 2 | `1 end` / `1 end`, exit 1 |
| f25 | `const xs = await many(); xs.length` | `2 end`, 0 | T29 after `2 end` | `2 end` / `2 end`, exit 1 |
| g01 | `new Holder([WORK])`, constructor stores the parameter in a field; `(await make()).jobs.length`; `Context.collect()` | `1 end`, 0 | T29 at `Context.collect()` (7:3), after `1` | `1 end` / `1 end`, exit 1 |
| g02 | the same with `Box<Promise<i32>[]>` | as g01 | as g01 | as g01 |
| g03 | a generator yields `WORK` after the `a339` discharge (`consume(job); stored.pop();`); `const r = g.next()`, never awaited | `false end`, 0 | T29 after `false end` | same / same, exit 1 |
| g04 | the same generator; `for (const h of gen()) { n++; }` | `1 end`, 0 | T29, stdout empty | same / same, exit 1 |
| g05 | the same generator; `gen().next().done` in place | `false end`, 0 | T29, stdout empty | same / same, exit 1 |
| g08 | `const f = (xs: Promise<i32>[]): i32 => xs.length; f([WORK])` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| g09 | `const f = async (p: Promise<i32>): Promise<i32> => 1; await f(WORK)` | `1 end`, 0 | T29, stdout empty, retained 2 | `1 end` / `1 end`, exit 1 |
| g11 | `(await mm()).size`, `mm(): Promise<Map<i32, Promise<i32>>>` | `1 end`, 1 | no trap, retained 1 | `1 end` / `1 end`, exit 1 |
| g12 | `(await work(1).then((v: i32): Promise<i32>[] => [WORK])).length` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| g13 | `(await grid()).length`, `grid(): Promise<Promise<i32>[][]>` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| g17 | `fs[0](WORK)`, `fs: ((p: Promise<i32>) => i32)[]` | `1 end`, 0 | T29, stdout empty | `1 end` / `1 end`, exit 1 |
| g19 | `const xs = await many(); const first = xs[0];` | `end`, 0 | T29 after `end` | `end` / `end`, exit 1 |

f16 and g11: a `Map` releases its values at `Context.free` or at a
collection (§172). Neither program runs one, so the count stays until
the Context ends (invariant 2). g01 shows the same holder kind with a
collection.

### 1.2 A pending task

The same forms with a task that suspends first:

```ts
async function late(v: i32): Promise<i32> { await Context.suspend(); print(`late ${v}`); return v; }
async function lateFail(): Promise<i32> { await Context.suspend(); print("late fail"); throw new Error("late"); }
```

| Id | Form | Completed | Failed |
|---|---|---|---|
| p01 | f01 | `2`, `main end`, `late 1` | the same output with `late fail`, then trap 29 at the `throw` in `lateFail` |
| p03 | f03 | `main end`, `late 1` | the same, trap 29 at the `throw` |
| p06 | f06 | `late 1`, `2`, `main end` | `late fail`, trap 29 at the `throw`, retained 2 |
| p10 | f10 | `late 1`, `1`, `main end` | the same output, trap 29 at the `throw` |
| p12 | f12 | `2`, `main end`, `late 1` | the same output, trap 29 at the `throw` |
| p20 | f08 | `1`, `main end`, `late 1` | the same output, trap 29 at the `throw` |
| p30 | g03 | `false`, `main end`, `late 1` | the same output, trap 29 at the `throw` |

A dropped pending task runs to its end. If it fails and no `await`
observed it, it traps 29 at its `throw` (§116.1 rule 4). This is the
behaviour that §186 part 6 measured for a freed holder. `node` prints
the same lines and exits 1. For p01, p03, and p12 `node` prints
`late` first; that order difference is the `Context.suspend()` base
model (`s186-then-measurement.md` 2.1), not this item.

### 1.3 Forms that the pin rejects

| Id | Form | `subscript check` at the pin | `node` completed |
|---|---|---|---|
| c01 | `WORK;` | S013 at the call | `end` |
| c02 | `const h = WORK;` | S013 at the call | `end` |
| c03 | `const xs = many2();`, `many2(): Promise<i32>[]`, only `.length` read | S013 at the call | `2 end` |
| f04 | `many2().length` | S013 at the call | `2 end` |
| f09 | `function count(p: Promise<i32>): i32 { return 1; }` | S013 at the parameter | `1 end` |
| f11 | `const [a, b] = many2(); await a;` | S013 at the call | `1 end` |
| f13 | `for (const h of many2()) { n++; }` | S013 at the call | `2 end` |
| f14 | `h.jobs = [WORK]` in an async function | S013 at the element | `1 end` |
| f17–f19, f24 | `function* gen(): Generator<Promise<i32>> { yield WORK; }` | S013 at the yield operand | as the program |
| f20 | a synchronous function returns `FixedArray<Promise<i32>, 2>` | S013 at each element | `2 end` |
| g06 | `async function pair(): Promise<FixedArray<Promise<i32>, 2>> { return [WORK, work(2)]; }` | S013 at each element | `2 end` |
| g10 | a method with a `Promise<i32>` parameter that it does not await | S013 at the parameter | `1 end` |
| g15 | `function* gen(xs: Promise<i32>[])` reads `xs.length` | S013 at the parameter | `1 end` |
| g16 | f09 called through a function value | S013 at the parameter | `1 end` |
| f15 | a field of `Promise<i32> \| null` | S011 | — |
| f21 | a `@ValueType` field of `Promise<i32>` | S100 (value-class field set) | — |

Programs that await every handle and that the pin rejects (false
rejections; `node` prints the same output in each):

| Id | Form | Pin |
|---|---|---|
| h01 | `h.job = work(1); print(\`${await h.job}\`)` (a field) | S013 at the call |
| h02 | `G = work(1); print(\`${await G}\`)` (a module global) | S013 at the call |
| h03 | `yield work(1)`; the consumer awaits each value | S013 at the yield operand |
| h04 | a local `FixedArray<Promise<i32>, 2>` returned, both elements awaited by the caller | S013 at each element |
| k10 | `function count(xs: Promise<i32>[]): i32 { return xs.length; }`; the caller then awaits `Promise.all(xs)` | S013 at the parameter |
| k11 | `const [a, b] = many2(); print(\`${await a} ${await b}\`)` | S013 at the call |

## 2. What S013 checks at the pin

The checker keeps a list of *origins* per function body. An origin is
discharged by one of a fixed set of uses. An origin that is not
discharged at the end of its body is S013 (`bodies.rs`
`check_function`, `expr/lambda.rs` `check_lambda_body`).

1. **Origins.**
   - A call whose result type is `Promise<T>` or `Promise<T>[]`
     (`Type::carries_async_handle`; `track_async_call_result`): a source
     function, an indirect call, a method, a completion function, a
     file-module function.
   - An async call, `Promise.all`, `then`/`catch`/`finally`, and a
     task group.
   - A function or method parameter of type `Promise<T>` or
     `Promise<T>[]` (`bind_params`). A constructor parameter and a
     lambda parameter have no origin.
2. **Flow** (`expr_async_origins`). A local, an array literal, a spread
   literal, an index read, a field read, a cast, and a conditional
   carry the origins of their operands. A local declaration and a
   `for…of` binding take the origins of the initializer or the subject.
   A pattern binding reads the hidden pattern source local, which
   carries no origin.
3. **Discharge** (`handle_async_origins`). The operand of an `await`;
   an argument whose parameter type is `Promise<T>` or any array; a
   `return` of `Promise<T>` or any array (function or lambda); the
   operand of `Promise.all`; the receiver of `then`/`catch`/`finally`.
   A local assignment replaces the origins of the local. An index store
   into a local array adds them. A field store, a global store, a
   `yield`, and a `FixedArray` return discharge nothing.
4. **One await discharges a whole value.** `await xs[0]` discharges
   every origin that `xs` carries. The rule is "at least one await
   through the value", not "an await of each handle".

Why each form of 1.1 escapes:

- An `await` result has no origin (f01–f03, f10, f12, f23, f25, g13,
  g19). `check_await` returns `AsyncHandleAwait` or an async call, and
  `expr_async_origins` gives no origin for either.
- A lambda parameter has no origin, and a `then` callback is a lambda
  (f06–f08, g08, g09, g12, g17).
- A call result of `Promise<T>[][]`, `FixedArray`, or `IterResult` is
  not an origin (f05, g05).
- A constructor parameter has no origin (g01, g02).
- A handle passed to `Map.set`, `push`, or a helper is discharged at
  the argument (f16, f22, g03–g05).

The pin false rejections of 1.3 follow from item 3: a field or global
store, a `yield`, and a `FixedArray` return discharge nothing, and a
pattern source local carries no origin.

## 3. Options

### 3.1 The prototypes

Each prototype is a checker change behind an environment variable. LIR,
codegen, the runtime, and the interpreter are not changed.

- **A, static rule.** `Type::holds_async_handle` is true for a handle
  and for a dynamic array, a `FixedArray`, or an `IterResult` whose
  element holds one, at any depth. New origins: each `await` result
  whose type holds a handle (the checked value is wrapped in
  `AsyncHandleTransfer`, which the lowering reads through), each call
  result whose type holds a handle, and each function, constructor,
  and lambda parameter whose type holds one. The argument, `return`,
  and lambda-return discharge sites also accept a type that holds a
  handle. The hidden pattern source local carries the origins of its
  initializer. The discharge rule stays "at least one await".
- **A+store.** A, and a store into a field or a module global
  discharges the stored origins.
- **B, runtime rule only.** The checker does not report
  `AsyncHandleUnawaited`. `TaskGroupUnjoined` stays. The runtime is
  the pin runtime.
- **C, statement rule.** B, and an expression statement whose value
  type holds a handle is S013.

### 3.2 Forms of part 1 that each option rejects

| Id | Pin | A | A+store | B | C |
|---|---|---|---|---|---|
| f01, f02, f03, f05, f06, f07, f08, f12, f25, g08, g09, g12, g13, g17, g19 | accept | S013 | S013 | accept | accept (f02: S013) |
| p01, p03, p06, p12, p20 | accept | S013 | S013 | accept | accept |
| f10, p10, f23 | accept | accept | accept | accept | accept |
| f16, g11 (`Map`), f22 (global push), g01, g02 | accept | accept | accept | accept | accept |
| g03, g04, g05, p30 (generator, `a339` form) | accept | accept | accept | accept | S013 at `stored.pop();` |
| c01 | S013 | S013 | S013 | accept | S013 |
| c02, c03, f04, f09, f13, g10, g15, g16 | S013 | S013 | S013 | accept | accept |
| f11, k11 | S013 | accept | accept | accept | accept |
| f14, h01, h02 | S013 | S013 | accept | accept | accept |
| h03, f17–f19, f24 | S013 | S013 | S013 | accept | accept |
| h04 | S013 | accept | accept | accept | accept |
| f20, g06, g07 | S013 at the elements | S013 at the consumer | S013 at the consumer | accept | accept |
| k10 | S013 | S013 | S013 | accept | accept |
| k08: `h.jobs = await many(); await h.jobs[0]` | accept | S013 | accept | accept | accept |

Controls that each option accepts (each awaits every handle, and the
three tiers print the `node` output under A): `const [a, b] = await many()`
with both awaited; a `for…of` that awaits each element; a `then`
callback that passes the array to `Promise.all`; a lambda that returns
its handle parameter; `await (await nested())`; an `await` result
returned; each element awaited by index.

Facts from the table:

- A covers the forms of 1.1 whose value is dropped with no await
  through it. It does not cover f10, p10, and f23: one await through
  the value discharges it, and the other element is dropped. "An await
  of each handle" is not decidable for a dynamic array, whose length
  is a run-time value.
- A does not cover a `Map` value, a value pushed into a global, or an
  `IterResult` of a generator `next()` call. The handle is discharged
  at the argument (`Map.set`, `push`, `consume`) before it is dropped.
- A adds one false rejection (k08) that A+store removes. A+store also
  removes the pin false rejections h01 and h02. Neither removes h03
  (`yield`) or k10 (a parameter that is read but not awaited).
- C rejects `stored.pop();`: a removal statement whose result holds a
  handle (§171 rule 5 releases it at the statement).

### 3.3 Corpus

`subscript check` over 461 entries (`corpus/accept` files and
directories, `corpus/warn`, the trap corpus), each with the mirror or
module option that the pin needs:

| Option | Entries rejected |
|---|---|
| pin | 0 |
| A | 0 |
| A+store | 0 |
| B | 0 |
| C | 10: `a162`, `a338`, `a339`, `a346`, `t87`, `t88`, `t92`, `t93`, `t105`, `t106` (`pop()`, `shift()`, or `fill()` as a statement) |

`subscript check` output over the 392 files of `corpus/reject` and
`examples`: A and A+store change no output. B accepts five reject
entries: `r100`, `r105`, `r157`, `r380`, and `r402`. Each runs in the
three tiers with the `node` output (`r157`: `not awaited`; `r402`:
`1`; the others print nothing) and 0 retained tasks.

Under A, the 53 accept entries that name `Promise<` and run in the
interpreter print their golden in the three tiers.

### 3.4 Checker cost

Release CLI, `subscript check` of each of the 350 accept entries in
series, best of three:

| Binary | Total |
|---|---|
| pin | 1.505 s |
| prototype, no option | 1.503 s |
| A | 1.494 s |
| A+store | 1.499 s |
| B | 1.508 s |

The differences are within the run-to-run spread (up to 0.04 s).
A dense program, 1,000 async functions that each take two counted
`await` results, destructure one, iterate one, and call a lambda with a
handle-array parameter, best of three: pin 100.7 ms, prototype with no
option 102.8 ms, A 107.4 ms (1.07 of the pin).

### 3.5 What the runtime already does (option B)

At the pin, the release of a counted value traps 29 if it frees a
failed task that no `await` observed (§171 rule 10, §172 rule 4, §176
rule 5). Under B, the failed variant of each program c01–c03, f01–f25
(except f15, f16, and f21), g01–g05, and p01–p30 traps 29 in the three
tiers, at the last release of the task (a completed task) or at the
`throw` (a pending task). The failed variants of g06, g07, g10, g15,
and g16 were not run under B. A completed task that is dropped is silent, as in `node`. The
`Map` holder of f16 and g11 keeps the count until `Context.free` or a
collection.

The difference from `node` is the time of the report: this language
traps at the release, so the output can be shorter (f01: empty, where
`node` prints `2 end` and then reports the rejection). C8 records this
divergence (§116.1 rule 4b).

## 4. `tsc`

`tsc` accepts every program of this note, the completed and the failed
variant, except `g14`: `await (await nested()).then(…)` is TS2339,
"Property 'then' does not exist on type 'number'". `tsc` types `await`
of `Promise<Promise<number>>` as `number`. f02 and f03 contain the same
`await nested()`. `node` flattens f02, f03, and f07, so they are not
comparable.

## Decisions a contract needs

1. Where the drop of a handle inside a value is caught:
   - A or A+store: extend S013 to the origins of 3.1. Cost 1.07 on the
     dense program; no corpus entry moves.
   - B: remove the S013 drop rule; the release trap of §171, §172,
     §176, and §186 part 6 reports a failed dropped task. Five reject
     entries change class. This reverses §70.1 decision 2 and the §186
     owner decision.
   - C: B with a statement rule. It rejects 10 corpus entries unless
     the removal methods are exempt; the exempt form is not measured.
2. If A: whether a store into a field or a module global discharges
   (A+store; h01, h02, k08).
3. If A: the residual forms, stated as a policy. A value whose handle
   is discharged at an argument (`Map.set`, `push`, a helper) and then
   dropped by the holder, and an array with one element awaited, stay
   runtime cases (f10, f16, f22, f23, g03–g05).
4. The pin false rejections (h01–h04, k10, k11): A closes k11 and h04;
   A+store also closes h01 and h02; B closes all six.
5. If B or C: the reject entries `r100`, `r105`, `r157`, `r380`, and
   `r402` become accept or trap entries, and C8 names the change.

## Appendix: helper declarations

```ts
async function many(): Promise<Promise<i32>[]> { return [WORK, work(2)]; }
async function nested(): Promise<Promise<i32>> { return WORK; }
function many2(): Promise<i32>[] { return [WORK, work(2)]; }
function grid(): Promise<i32>[][] { return [[WORK]]; }
let stored: Promise<i32>[] = [];
function consume(job: Promise<i32>): void { stored.push(job); }
function* gen(): Generator<Promise<i32>> { const job = WORK; consume(job); stored.pop(); yield job; }
class Holder { jobs: Promise<i32>[]; constructor(jobs: Promise<i32>[]) { this.jobs = jobs; } }
async function make(): Promise<Holder> { return new Holder([WORK]); }
```

Each `main` ends with `print("end")` (`print("main end")` in 1.2).

## Implementation

Contract: `compiler.md` §188, pin `5149dfb3`. The checker implements
option "A+store" of part 3.1. LIR, codegen, the runtime, and the
interpreter do not change.

### Checker

- `Type::holds_async_handle` (rule 1): `Promise<T>`, and `T[]`,
  `FixedArray<T, N>`, or an iterator result whose element holds a
  handle, at any depth. `Map`, `Set`, and `Generator` do not hold one.
- Origins (rule 2):
  - a call result whose type holds a handle, at each site that the pin
    tracks (`track_async_call_result`): a source function, an indirect
    call, a method, a completion function, a file-module function;
  - an `await` result whose type holds a handle;
  - a parameter of a function, a method, a constructor, or a lambda
    (a `then`, `catch`, or `finally` callback included) whose type
    holds a handle;
  - the hidden storage of a declaration pattern and of a `for…of`
    pattern carries the origins of its initializer or subject.
- A built-in method result is not an origin: `pop()`, `shift()`,
  `next()` on a generator, and `Map.get` read a value that is already
  held. g03–g05 and `a339` stay accepted for this reason.
- Discharges (rule 3): the argument and `return` sites accept a type
  that holds a handle or a dynamic array. A store into a field, into a
  module global, or into an element under a field or a global
  discharges the stored origins. A store into an element of a local,
  at any index depth, adds the origins to the local.
- S013 (rule 4): one function reports each origin that its body does
  not discharge, for a function, a lambda, and a constructor body. At
  the pin a constructor body reported no origin.

### Corpus

| Id | Pin `5149dfb3` (three tiers) | HEAD |
|---|---|---|
| `r414-await-result-handle-array-in-place` | accepted, `2`, `end` | S013 at 10:13 |
| `r415-destructured-await-result-dropped` | accepted, `end` | S013 at 10:19 |
| `r416-then-callback-handle-parameter` | accepted, `2 end` | S013 at 10:32 |
| `r417-constructor-handle-parameter` | accepted, `1 end` | S013 at 10:15 |
| `r418-nested-handle-array-call-result` | accepted, `1 end` | S013 at 10:12 |
| `a364-handle-field-store-awaited` | 1 S013 at 14:11 | `0`, `1`, `end` |
| `a365-handle-global-store-awaited` | 2 S013 at 8:31, 8:40 | `1 2`, `end` |
| `a366-fixed-array-handle-return` | 2 S013 at 8:46, 8:55 | `1 2`, `end` |
| `a367-destructured-call-result-awaited` | 1 S013 at 9:18 | `1 2`, `end` |

`r415` binds element 0 and drops element 1, and no binding is awaited.
A destructured `await` result with one binding awaited is accepted
(f10, rule 5). `node` v24.18.0 prints each `.expected` of the four
accept entries; each is `js-comparable: yes`. `tsc` 5.9.2 accepts the
nine programs. The three tiers print the `.expected` of each accept
entry.

`subscript check` over the accept, warn, trap, reject, and example
files: the output of HEAD and of the pin differs only for the nine new
entries. The LIR text golden (`codegen/tests/lir-goldens/corpus.txt`)
adds the four new async accept entries; the text of each other entry
does not change.

### Tests

- `compiler/tests/async_handle_origins.rs`: each origin kind and each
  discharge, with a control in the same shape. Each dropped program
  has exactly one S013 at the marked origin site. The runtime cases
  f10, f16, f22, f23, and g03–g05 check; h03 and k10 stay S013.
- `codegen/src/interpreter/counted_measurement_tests.rs`:
  - The inline holder rows drop an `await` result (`await outer;`,
    `await make();`), which rule 2 rejects. Each row now binds the
    result and awaits it under `if (false)`, so the checker sees one
    await and the run still drops the value.
  - The fresh index rows (`stored[0]=work()`, `o.jobs[0]=work()`,
    `n[0][0]=work()`) were S013 at the pin. Rule 3 accepts them; the
    test runs each in the three tiers with 0 retained tasks.
- `compiler/tests/counted_capture_block.rs`: the `Holder` constructor
  body called `g()` and dropped the handle. The body is now empty, so
  the test reports only its S009.
- `compiler/tests/generic_tsc_matrix/destinations.rs`: 1,494 omitted
  concrete instances (1,488 at the pin). The six new ones have a
  `FixedArray<Promise<i32>, 2>` parameter that their body does not
  discharge.

### Checker cost

Release CLI, `subscript check` of each accept entry in series, best of
three, with the mirror and module options that each entry needs:

| Set | Pin | HEAD |
|---|---|---|
| 351 entries that both accept | 1.510 s | 1.513 s |
| 355 entries (HEAD) | — | 1.526 s |

Dense program: 1,000 async functions that each destructure an `await`
result, iterate an `await` result, and call a lambda with a
handle-array parameter. Best of three: pin 114.2 ms, HEAD 117.8 ms
(1.03 of the pin).

### Open

1. A built-in method result is not an origin (see Checker). Rule 2
   says "any callee".
2. `specs/tracking/s172-reference-holders.md` names the test
   `measured_fresh_index_positions_still_have_no_accepted_form`. Its
   name is now
   `measured_fresh_index_positions_are_accepted_and_release_every_task`.
