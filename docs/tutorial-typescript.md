# subscript for TypeScript developers

subscript writes the logic that runs inside a native application. The
application owns the process and the main loop. It calls the functions
your script exports. A game engine is the first example, but the same
shape fits audio, simulation, creative tools, and embedded control.

The syntax is a subset of TypeScript. Every accepted program also
type-checks under stock `tsc`, so tsserver gives you completion,
rename, and go-to-definition with no plugin. The semantics below the
syntax are not JavaScript's. Values have C data layout. Integers have
fixed widths. No collector runs unless the host or the script calls
it. The compiler rejects the dynamic
patterns that it cannot compile to predictable machine code.

This page is the list of what changes. Every program and every output
on this page was run against the repository as committed, and every
program type-checks under stock `tsc --strict`.

## The short version

| In TypeScript | In subscript |
|---|---|
| `number` | `i8`–`i64`, `u8`–`u64`, `f16`, `f32`, `f64` |
| Structural types | Nominal types; same shape is not the same type |
| `class` | Reference class (`new`, heap) or `@ValueType` value class (copied) |
| `undefined`, `T \| U` | `Ref \| null` only, narrowed before use |
| `enum` of strings | `type Mode = "fast" \| "safe"`, closed and nominal |
| Garbage collection | Nothing to write by default: the host collects at a step boundary. `Context.free` and `using` are optimizations. Nothing collects unbidden |
| `throw` / `try` | The seven `Error`-family classes only; `finally` runs on each exit; faults are traps, which no `catch` or `finally` stops |
| Event loop, `Promise` | Host-stepped suspension; `Promise<T>` is an annotation |
| `Worker` with structured clone | `Worker.spawn` with copied, typed messages |
| A program with a top level | Exported entry points the host calls |
| `string` of UTF-16 units | `string` of UTF-8 bytes |
| npm | Sibling `./file` imports only |

## Setup

From the repository root:

```sh
cargo build --release -p subscript-cli
alias subscript=target/release/subscript
```

```ts
export function main(): void {
  print("hello from subscript");
}
```

```sh
$ subscript run hello.ts
hello from subscript
```

`run` executes the program on the development tier, a JIT compiler.
`print` writes to a sink the host owns. There is no `console`.

## Numbers have widths

JavaScript has one numeric type, a 64-bit float. Here every numeric
type names its width: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`,
`u64`, `f32`, `f64`, and storage-only `f16`. A field's width is part
of the C layout, so a default numeric type has no correct answer.
Bare `number` is rejected, and the diagnostic shows both spellings:

```text
error[S007]: bare `number` is rejected; there is no default numeric type — use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)
 --> bare.ts:2:16
  |
2 |   const count: number = 3;
  |                ^
  = rule: Bare `number` is rejected; sized numeric types are mandatory.
  = TypeScript accepts:
  |   const count: number = 3;
  = subscript:
  |   const count: i32 = 3;
  = why: `number` is a 64-bit float with no C width, so every declaration names one of the sized types. (collisions.md C3)
error: 1 error(s)
```

A literal takes the type of its context, and it must fit that type. A
conversion is explicit, with `as`, and an integer conversion truncates
like a C cast. Integer arithmetic wraps; it does not widen to a float.

```ts
export function main(): void {
  const wide: i64 = 4000000000;
  const narrow: u8 = 255;
  const truncated: u8 = wide as u8;
  const scaled: f32 = 1.0 / 3.0;
  print(`wide=${wide} narrow=${narrow} truncated=${truncated} scaled=${scaled}`);
  const wrapped: u8 = (narrow + 1) as u8;
  print(`wrapped=${wrapped}`);
}
```

```text
wide=4000000000 narrow=255 truncated=0 scaled=0.33333334
wrapped=0
```

`f32` prints as an `f32`. `1.0 / 3.0` is `0.33333334`, not the `f64`
value JavaScript prints. `f16` holds storage only: convert it to `f32`
or `f64` before arithmetic.

## Classes are nominal, and there are two kinds

Two classes with the same fields are two types. Passing one where the
other is declared is rejected. Structural substitution is what makes a
C-identical layout impossible to guarantee, so the language does not
have it. A class also has exactly the members it declares: no property
appears later, and no prototype changes.

A plain `class` is a **reference class**. `new` allocates it in the
Context, and assignment copies the reference, as in TypeScript.

`@ValueType` marks a **value class**. It has C struct layout, and it is
copied on assignment and on every call. Nothing aliases it, so no
second name observes a write. A value class cannot use `extends`.

```ts
@ValueType
class Vec2 {
  x: f32;
  y: f32;
  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
  length2(): f32 {
    return this.x * this.x + this.y * this.y;
  }
}

class Counter {
  static made: i32 = 0;
  count: i32 = 0;
  constructor() {
    Counter.made += 1;
  }
  get doubled(): i32 {
    return this.count * 2;
  }
  set doubled(value: i32) {
    this.count = value / 2;
  }
}

export function main(): void {
  const a: Vec2 = new Vec2(3.0, 4.0);
  const b: Vec2 = a;
  b.x = 0.0;
  print(`a.x=${a.x} b.x=${b.x} len2=${a.length2()}`);
  const c: Counter = new Counter();
  c.doubled = 10;
  print(`count=${c.count} doubled=${c.doubled} made=${Counter.made}`);
}
```

```text
a.x=3 b.x=0 len2=25
count=5 doubled=10 made=1
```

`b` is a copy of `a`, so the write to `b.x` leaves `a.x` at 3.

The class body carries more than fields. Field initializers run on
every construction. Static fields and static methods live on the class.
Accessors are sugar for methods: `get name()` becomes a method that a
read calls, and `set name(v)` becomes a method that a statement write
calls. A value class declares read accessors; only a reference class
declares an instance write accessor.

Two more members are available. A class declares an index signature
(`[index: u32]: T`, with `readonly` for read-only access). A method on
a non-generic class takes type parameters, and each call states them:

```ts
class Pair {
  first<T>(values: T[]): T {
    return values[0];
  }
}

export function main(): void {
  const p: Pair = new Pair();
  print(`${p.first<i32>([7, 8])} ${p.first<string>(["a", "b"])}`);
}
```

```text
7 a
```

Explicit type arguments are required. The compiler emits one function
per argument list, which is how a call stays a direct call.

## `Ref | null` is the only union, and narrowing is mandatory

There is no `undefined`. There is no optional property that means
absence. The only union is `Ref | null`, where `Ref` is a reference
class, an opaque handle, a function type, or a boundary struct
pointer. A member access needs narrowing first. `i32 | null` is
rejected: a scalar has no null. This is the control-flow narrowing TypeScript already
taught you. Here the compiler requires it.

`a ?? b` works when `a` has type `Ref | null`. It reads `a` once, and
it evaluates `b` only when `a` is `null`.

`==` and `!=` mean `===` and `!==` here; `==` and `===` are one operator.
No coercion happens: `tsc` rejects comparisons of unrelated types, and the language has no `undefined`.
Where JavaScript gives `undefined` (a miss of `Map.get` or `find`, a finished generator), this language gives `null`.
Test such a value with `== null`, which gives the same result in both.
See [`compiler.md` §144](../specs/blocks/compiler.md).

```ts
class Node {
  value: i32;
  next: Node | null;
  constructor(value: i32, next: Node | null) {
    this.value = value;
    this.next = next;
  }
}

function find(head: Node | null, value: i32): Node | null {
  let cursor: Node | null = head;
  while (cursor != null) {
    if (cursor.value == value) {
      return cursor;
    }
    cursor = cursor.next;
  }
  return null;
}

export function main(): void {
  const head: Node = new Node(1, new Node(2, null));
  const fallback: Node = new Node(-1, null);
  const found: Node = find(head, 2) ?? fallback;
  const missing: Node = find(head, 9) ?? fallback;
  print(`found=${found.value} missing=${missing.value}`);
}
```

```text
found=2 missing=-1
```

Optional chaining exists in two positions, because every other
position produces `undefined`. `x?.v` is legal as the whole left
operand of `??`, as in `x?.next?.v ?? 0`. `x?.m();` is legal as a
statement whose last step is a call. Elsewhere the compiler rejects it.

A `@Descriptor` class is the one place where an omitted member is a
state of its own. It is a data-only class built from an object literal,
which suits the option bags that C APIs take. `name!: T` is required,
and `name?: T = default` has a default:

```ts
@Descriptor
class Limits {
  maxItems!: i32;
  label?: string = "default";
}

function describe(limits: Limits): string {
  return `${limits.label}:${limits.maxItems}`;
}

export function main(): void {
  print(describe({ maxItems: 4 }));
  print(describe({ maxItems: 9, label: "tuned" }));
}
```

```text
default:4
tuned:9
```

## Closed string unions replace string enums

A declared alias of string literals is a nominal, closed type. A
non-member, an inline union, and a value from another alias of the
same members are all rejected. A `switch` over the alias without a
`default` must name every member exactly once. The compiler then knows
the switch is exhaustive, so a function that returns from every arm
needs no trailing return:

```ts
type Mode = "fast" | "safe" | "debug";

function budget(mode: Mode): i32 {
  switch (mode) {
    case "fast":
      return 1;
    case "safe":
      return 4;
    case "debug":
      return 16;
  }
}

export function main(): void {
  print(`fast=${budget("fast")} safe=${budget("safe")} debug=${budget("debug")}`);
}
```

```text
fast=1 safe=4 debug=16
```

Add a member to `Mode` later, and this function stops compiling until
you handle it. `unreachable()` marks a path that no input reaches. It
is legal as a statement, it counts as a diverging path for return-flow
analysis, and it traps if execution arrives there.

Numeric `enum` also exists, and it lowers to a C enum.

## Memory is explicit

No collector runs on its own. This is a design invariant, not a
setting. The recommended pattern puts the release work on the host:

1. **The script writes no release code by default.** A handle and a
   generator release themselves. Every other value stays until a
   collection. A program that never frees and never collects is
   correct; it only holds more memory.
2. **The host collects at a step boundary**, for a game once per
   frame. It calls `subscript_rt_ctx_collect` between script calls, at
   script depth 0, on the thread of the Context (§18.2d). The call is
   the same mark-and-sweep as `Context.collect()`. Its cost grows with
   the live data, not with the allocations since the last collection
   (§22.2, §22.4 criterion 4).
3. **`Context.free`, `using`, and a script `Context.collect()` are
   optimizations.** Use them where a measured peak matters: a large
   temporary, or a loop that allocates many objects in one call. A
   host collection runs between calls, so it does not lower a peak
   inside one call. Use `using` where a resource needs its release at
   a known point.
4. **A value that crosses to the host follows the host's protocol.**
   The script cannot free a host object behind a handle; the host
   keeps it valid (§142). Callback userdata that the host holds stays
   a collection root while its registration lives. A registration with
   the default lifetime lives as long as the Context (§14.4b, §111).

Each value follows one of two release rules:

- **Counted values.** A handle (`Promise<T>`) and a generator own a
  suspended frame. A dynamic array of such values is also counted, at
  any depth (§171 rule 1). The compiler counts the holders of a
  counted value. When its last holder releases it, the value is freed
  at once, with no collection (§70 rule 3, §171 rule 4, §176 rule 3).
- **Context memory.** Every other allocation is in the **Context**,
  the arena the host creates and releases. This includes the
  allocations that the compiler makes where the source has no `new`.
  Context memory stays until a collection, `Context.free`, or the
  Context release.

Three calls release Context memory. Each runs only where it is
written:

- The host's `subscript_rt_ctx_collect` collects what script references
  no longer reach. This is the default release path.
- `Context.free(value)` releases one reference object at once.
- `Context.collect()` in a script runs the same collection inside a
  script call.

Dropping the last reference to Context memory frees nothing by itself.
Dropping the last holder of a counted value releases it. In the table
below, `Context.collect()` stands for either collection call.

| Value | Who allocates it | What releases it |
|---|---|---|
| A reference object, `Map`, or `Set` | The program, with `new` | `Context.free`, `Context.collect()`, or the Context release |
| A string or an array of uncounted values | The compiler and the runtime: an array literal, a concatenation, a template, a method result | `Context.collect()` or the Context release |
| The capture environment of an async arrow (§181) or of a `then`/`catch`/`finally` callback (§186 rule 4) | The compiler, at each evaluation of a capturing lambda | `Context.collect()` after no held value and no live frame reaches it, or the Context release |
| The captures of another synchronous lambda | The compiler, in the frame of the defining function | The exit of that frame (§118) |
| A handle, including the handle of `then`, `catch`, or `finally` | An async call or one of those forms | The release by its last holder; an unfinished frame is freed when it finishes (§70, §172 rule 6) |
| A generator | A generator call | The release by its last holder (§176) |
| A dynamic array of handles or generators | An array literal, a spread, or an array method | The release by its last holder, which releases each element (§171 rule 4) |
| A counted value in a class field or a `Map` value | A field store or `Map.set` | `Context.free` of the holder, or the `Context.collect()` that reclaims it; also a replacing store, `Map.delete`, or `Map.clear` (§172) |

A program that never collects is **correct**. It holds more memory
until the host releases the Context. Each capturing callback allocates
one environment, and the host collection at each step boundary
reclaims the environments that nothing reaches.

The optimization forms follow. `Context.free` ends an allocation at
once, and a script `Context.collect()` reclaims inside the call. This
program needs neither: a host collection after `main` reclaims `temp`
and each `scratch`. They lower the peak of the call:

```ts
class Frame {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

export function main(): void {
  const kept: Frame = new Frame(1);
  const temp: Frame = new Frame(2);
  Context.free(temp);
  for (let i: i32 = 0; i < 3; i += 1) {
    const scratch: Frame = new Frame(i);
    Context.free(scratch);
  }
  Context.collect();
  print(`kept=${kept.id}`);
}
```

```text
kept=1
```

`using` calls `[Symbol.dispose]()` at each scope exit, in reverse
declaration order (§60). Use it where a resource needs its release at
a known point. The hook releases what the class holds. The object
itself is Context memory like any other value. The class declares the
hook as in TypeScript's explicit resource management:

```ts
class Buffer {
  id: i32;
  constructor(id: i32) {
    this.id = id;
    print(`open ${id}`);
  }
  [Symbol.dispose](): void {
    print(`close ${this.id}`);
  }
}

export function main(): void {
  using first = new Buffer(1);
  {
    using second = new Buffer(2);
    print("inner work");
  }
  print("outer work");
}
```

```text
open 1
open 2
inner work
close 2
outer work
close 1
```

The compiler warns where it proves unbounded growth. `W001` flags an
allocation that a loop repeats and that neither escapes the iteration
nor is released. That growth is inside one call, where a host
collection does not reach. `W002` flags a local read after `Context.free`. `W003`
flags fresh callback userdata registered in a loop. `W004` flags a
value copy that a function writes through and never reads. Warnings do
not fail a build. `subscript check --deny-warnings` makes them fail in
CI.

## Exceptions, result values, and traps

`throw`, `try`, and `catch` are in the language, with a narrower
surface than in JavaScript. A thrown value is an object of the
`Error` family: `Error`, `SyntaxError`, `TypeError`, `RangeError`,
`ReferenceError`, `EvalError`, or `URIError`. The catch binding has two
uses: `instanceof`, which narrows it to the class, and `throw`, which
rethrows it. A `finally` block runs once on each exit of its `try` and
`catch` blocks; a `return` or `throw` in it replaces the earlier
completion, as in JavaScript. A trap runs no `finally` block (§180). A
`try` block and a `finally` block can hold `await` or `yield` (§116.1
rule 7, §180). `throw 42` and `catch (e: any)` are
rejected with `S010`.

The finalizer completes its `await` before the function returns from the catch block:

```ts
async function recover(): Promise<i32> {
  try {
    throw new Error("retry");
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
    return 7;
  } finally {
    print("cleanup start");
    await Context.suspend();
    print("cleanup end");
  }
}

export async function main(): Promise<void> {
  print(`result=${await recover()}`);
}
```

```text
caught retry
cleanup start
cleanup end
result=7
```

`JSON.parse<T>` returns a `T`. Malformed text raises `SyntaxError`
with the UTF-8 byte offset of the first byte the parser cannot accept.
A document that does not match `T` raises `TypeError`:

```ts
class Config {
  name: string;
  count: i32;

  constructor(name: string, count: i32) {
    this.name = name;
    this.count = count;
  }
}

export function main(): void {
  try {
    const config: Config = JSON.parse('{"name":"demo","count":');
    print(config.name);
  } catch (e) {
    if (e instanceof SyntaxError) {
      print(e.message);
    }
  }
  try {
    const config: Config = JSON.parse('{"name":"demo"}');
    print(config.name);
  } catch (e) {
    if (e instanceof TypeError) {
      print(e.message);
    }
  }
}
```

```sh
$ subscript run errors.ts
JSON.parse: invalid syntax at byte 23
JSON.parse: document does not match Config
```

An expected failure can also be a value. A lookup that finds nothing
returns `T | null`, and a function can return a result-shaped value
that the caller tests.

A fault is a **trap**. An index outside an array, an integer division
or remainder by zero, a `null` where an `as` narrowing promised a
reference, a use after `Context.free`, and a reached `unreachable()`
are traps. An exception that no handler catches becomes a trap at the
host entry. A trap records the rule, the message, and the source
position in the Context. It stops the current entry. The host then
reads what happened:

```sh
$ subscript run oob.ts
subscript: oob.ts:4:17: trap [index-out-of-bounds]: index 5 out of bounds for array length 3
```

A trap is not catchable in script. A fault is a defect to fix, and
the host decides what happens next. The host reads the rule, the
message, and the position through its C API. The Context stays
readable, so the host inspects state before it releases it.
`corpus/trap/` holds the trapping programs, each with the output it
produced before the fault.

## `async`/`await` without a scheduler

`async` and `await` are accepted, and they mean something narrower
than in JavaScript. **Nothing schedules the resumption.** A pending `await`
suspends the function. The computation continues when the host steps
it. There is no event loop and no `Promise` object at run time.
`Promise<T>` in an annotation is the `tsc` view of a Context-owned
handle.

```ts
let clock: i32 = 0;

async function settle(steps: i32): Promise<i32> {
  for (let i: i32 = 0; i < steps; i += 1) {
    clock += 1;
    await Context.suspend();
  }
  return clock * 10;
}

export async function main(): Promise<void> {
  const value: i32 = await settle(3);
  print(`after ${clock} steps: ${value}`);
}
```

```text
after 3 steps: 30
```

These forms are awaitable: `Context.suspend()`, a direct call of an
`async` function or `async` instance method, and a handle that an
earlier call produced. A call through a function value that returns a handle is also awaitable (§167).
`Promise.all` over an array of handles, `TaskGroup.join()`, and a call
of a host completion function are awaitable too (§166, §170, §178).
A named async function is a value. An async arrow can capture a `const`
local of an enclosing function; the arrow can then be stored, returned,
and called later (§181). A captured `let`, `var`, parameter, or `this`
is rejected with `S009`: copy the value into a `const` first.

The returned object holds the arrow after `makeJob` returns:

```ts
class Job {
  run: () => Promise<i32>;
  constructor(run: () => Promise<i32>) {
    this.run = run;
  }
}

function makeJob(): Job {
  const base: i32 = 7;
  return new Job(async (): Promise<i32> => {
    await Context.suspend();
    return base;
  });
}

export async function main(): Promise<void> {
  const job: Job = makeJob();
  print(`result=${await job.run()}`);
}
```

```text
result=7
```

A local, an array, a field, or a global can hold
a handle (§70.3 rule 2a). A handle can pass to another function.
Every handle a program creates must have
one awaited completion. `new Promise` and the statics other than
`Promise.all` do not exist.

`then`, `catch`, and `finally` on a handle create a new handle (§186).
A callback lambda can capture a `const`. A callback parameter has the
value type of the handle, or `Error` in a rejection callback; another
parameter type is rejected. A `catch` callback returns the
value type of its handle; another result type is rejected with `S013`.
Each capturing async arrow or callback of these forms allocates an
environment that stays until a collection; the handle is counted
([Memory is explicit](#memory-is-explicit)).
The callbacks run in the order that `node` gives:

```ts
async function value(n: i32): Promise<i32> {
  await Context.suspend();
  return n;
}

async function fails(): Promise<i32> {
  await Context.suspend();
  throw new Error("boom");
}

export async function main(): Promise<void> {
  const base: i32 = 10;
  const sum: Promise<string> = value(1)
    .then((v: i32): i32 => v + base)
    .then((v: i32): string => `sum ${v}`);
  print(await sum);
  print(`recovered ${await fails().catch((e: Error): i32 => e.message.length)}`);
  print(`${await value(2).finally((): void => { print("finally"); })}`);
}
```

```text
sum 11
recovered 4
finally
2
```

An async call runs the callee to its first await at the call. A callee
that never awaits completes at the call, and its handle carries the
result.

**Every `await` suspends its caller**, a completed handle included. The
await registers the caller as a continuation of the awaited work and
returns. It never resumes that work itself. When the awaited handle
completes, its registered continuations join a first-in, first-out
ready queue.

**Only the host runs that queue.** One host step makes the frames that
waited for a step runnable, after the jobs that are already ready, and
then runs the queue until it is empty. A frame that waits for a step
during that run waits for the next one. So a call never runs another
frame's continuation, and two held calls make progress together rather
than in the order their holder awaits them:

```ts
async function leaf(tag: string): Promise<i32> {
  print(`${tag}:leaf`);
  return 1;
}

async function chain(tag: string): Promise<i32> {
  print(`${tag}:start`);
  const value: i32 = await leaf(tag);
  print(`${tag}:end`);
  return value + 1;
}

export async function main(): Promise<void> {
  const a: Promise<i32> = chain("A");
  const b: Promise<i32> = chain("B");
  print("main:mid");
  print(`a=${await a} b=${await b}`);
}
```

```text
A:start
A:leaf
B:start
B:leaf
main:mid
A:end
B:end
a=2 b=2
```

That is JavaScript's order for the same program, and this language
reaches it without an event loop: the queue advances only inside the
host's step.

Two consequences follow. The unbudgeted host step has no work budget,
so a chain of completed awaits that never ends keeps it from returning;
`Context.suspend()` is the boundary a program uses to hand control back.
The budgeted step (`subscript_rt_ctx_async_step_budget`, §168) returns
after a stated number of dispatches.
And a host that sees no pending work has not proved that every call
finished: `subscript_rt_ctx_async_unfinished` counts the started
invocations with no completion, and the host operations that the host
did not complete yet (§178 rule 11).

An exception that leaves an async body completes its handle; the call
returns that handle (§116.1 rule 1).
An `await` of the failed handle raises the exception, so a `try`
around the `await` catches it (§116.1 rule 2):

```ts
async function fails(): Promise<i32> {
  throw new Error("boom");
}

export async function main(): Promise<void> {
  try {
    await fails();
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }
}
```

```text
caught boom
```

Each `await` of one failed handle raises the same exception object
(§116.1 rule 3). This example prints its message at each raise:

```ts
async function fails(): Promise<i32> {
  throw new Error("boom");
}

export async function main(): Promise<void> {
  const job: Promise<i32> = fails();
  print("after call");
  try {
    await job;
  } catch (e) {
    if (e instanceof Error) {
      print(`first ${e.message}`);
    }
  }
  try {
    await job;
  } catch (e) {
    if (e instanceof Error) {
      print(`second ${e.message}`);
    }
  }
}
```

```text
after call
first boom
second boom
```

If no `await` observes a failed handle, its last holder's release traps
(§116.1 rule 4; [`t66`](../corpus/trap/t66-unobserved-async-exception.ts)).
An exception that leaves a host-callable async export traps with
`TrapKind::UncaughtException` (29), because the export has no script holder
(§116.1 rule 5).

## Cooperative cancellation

A token holds a cancellation flag. A source sets that flag.
A task observes cancellation only when it runs and checks the token.
The check throws an `Error`; the caller catches it around the await.

```ts
class CancellationToken {
  cancelled: boolean = false;
  throwIfCancelled(): void {
    if (this.cancelled) { throw new Error("cancelled"); }
  }
}
class CancellationSource {
  token: CancellationToken = new CancellationToken();
  cancel(): void { this.token.cancelled = true; }
}
async function work(token: CancellationToken): Promise<void> {
  await Context.suspend();
  token.throwIfCancelled();
  print("work completed");
}
export async function main(): Promise<void> {
  const source: CancellationSource = new CancellationSource();
  const job: Promise<void> = work(source.token);
  source.cancel();
  try { await job; }
  catch (e) {
    if (e instanceof Error && e.message === "cancelled") { print("work cancelled"); }
    else { throw e; }
  }
}
```

```text
work cancelled
```

## Task groups

A `TaskGroup` holds tasks and waits for all their completions.
Its join reports the first failure in reaction order after every task finishes.
The group lives in one `const` local. A synchronous helper can borrow it.
Call `join()` in its declaring scope. Add no task after that call.
An unjoined scope exit traps if unfinished or failed tasks remain.
A group cannot live in a field, an array, a result, or a captured lambda.

```ts
async function work(tag: string): Promise<void> {
  await Context.suspend();
  print(tag);
}
function add(group: TaskGroup): void { group.add(work("second")); }
export async function main(): Promise<void> {
  const group: TaskGroup = new TaskGroup();
  group.add(work("first"));
  add(group);
  await group.join();
  print("joined");
}
```

```text
first
second
joined
```

## Coroutines

A `function*` coroutine yields typed values. The caller advances it,
one step per call, which suits per-frame work. `for...of` over a
generator is accepted:

```ts
function* positions(): Generator<i32> {
  let position: i32 = 0;
  for (let step: i32 = 1; step <= 3; step += 1) {
    position += step * 2;
    yield position;
  }
}

export function main(): void {
  for (const position of positions()) {
    print(`position=${position}`);
  }
}
```

```text
position=2
position=6
position=12
```

An early `break` closes the generator and runs the finalizer around its current
`yield` (§180 rule 7):

```ts
function* values(): Generator<i32> {
  try {
    yield 1;
    yield 2;
  } finally {
    print("generator cleanup");
  }
}

export function main(): void {
  for (const value of values()) {
    print(`value=${value}`);
    break;
  }
  print("loop end");
}
```

```text
value=1
generator cleanup
loop end
```

`Generator<T>.next()` gives the explicit form, with `done` and `value`
on the result, for a host that drives one step per frame.

## Workers are threads with copied messages

A `Worker` runs one named, module-level, synchronous function on an OS
thread with a **fresh Context**. Nothing is shared. A message is
copied into the receiving Context, so no reference crosses a thread.

```ts
class Job {
  from: i32;
  to: i32;
  constructor(from: i32, to: i32) {
    this.from = from;
    this.to = to;
  }
}

class Tally {
  count: i32;
  constructor(count: i32) {
    this.count = count;
  }
}

function countEven(inbox: Inbox<Job>, outbox: Outbox<Tally>): void {
  const job: Job | null = inbox.wait();
  if (job == null) {
    return;
  }
  let count: i32 = 0;
  for (let n: i32 = job.from; n < job.to; n += 1) {
    if (n % 2 == 0) {
      count += 1;
    }
  }
  outbox.post(new Tally(count));
}

export function main(): void {
  const left: Worker<Job, Tally> = Worker.spawn(countEven);
  const right: Worker<Job, Tally> = Worker.spawn(countEven);
  left.post(new Job(0, 100));
  right.post(new Job(100, 200));
  left.close();
  right.close();
  left.join();
  right.join();
  const a: Tally | null = left.poll();
  const b: Tally | null = right.poll();
  if (a != null && b != null) {
    print(`left=${a.count} right=${b.count} total=${a.count + b.count}`);
  }
}
```

```text
left=50 right=50 total=100
```

The rules that follow from the isolation:

- The entry is a named module function. It captures nothing: a capture
  names memory in the Context it came from.
- A message class holds sized numerics, booleans, enums, string-literal
  union aliases, value classes, top-level `string` fields, and
  top-level `FixedArray<string, N>` fields. A string travels as a copy
  of its bytes. Reference, growable-array, function, and nullable
  fields are not transferable.
- A worker handle belongs to the Context that spawned it. It is not a
  module global, a class field, an array element, a `Map` or `Set` type
  argument, or a lambda capture.
- `post` never blocks. `poll` never blocks and answers `null` when
  nothing is available. Worker-side `wait` blocks that worker's thread
  only. `close` then `join` shuts a worker down, and `join` reports a
  worker's trap to the parent.
- Post all independent work before the first `join`, or the work runs
  in sequence.

The host still owns the main loop. Workers are the language's own
threads for computation, not a way to call the host from a thread it
does not know.

## Exports are the host's entry points

There is no top-level program. The application calls what the entry
module exports. The entry module is the file you give to
`subscript run`, `check`, or `build`. A one-shot program exports
`main`. A frame-driven program exports entries such as `init`,
`update`, and `shutdown`, and the host calls them.

Each function that the entry module exports is a host entry, and it
must be host-callable. Three facts must hold. It is synchronous. It
returns `void`. Every parameter is a boundary scalar (a sized numeric
or a `boolean`) or an opaque handle from the host's C API. A
zero-argument `void` async function is host-callable too. A host entry
becomes the C symbol `subscript_export_<name>`. The entry module
exports functions only; any other export there is an error.

An export of any other module is for `import` only: other script
modules call it, and it gets no C symbol. The entry module can
re-export such a function under a host name:
`export { update as physicsUpdate } from "./physics";`.

Module-level variables live in the Context and persist between calls.
That is how per-frame entries share state:

```ts
class SessionState {
  distance: f32 = 0.0;
}

let session: SessionState | null = null;

export function init(): void {
  session = new SessionState();
}

export function update(dt: f32): void {
  if (session != null) {
    session.distance += dt;
  }
}
```

The complete version, where `update` reads the frame's real inputs
from the host's declared API, is
[`examples/host/game.ts`](../examples/host/game.ts). The calling side
is step 6 of the [C/C++ tutorial](tutorial-c-cpp.md).

### Values that come from the host

A value comes from the host as an entry parameter, as a foreign-call
result, as a field of a struct that the host fills, or as a
completion. Each kind has one owner and one release
rule. The section numbers refer to
[`specs/blocks/compiler.md`](../specs/blocks/compiler.md).

| Value | Owner | What the script does |
|---|---|---|
| A host handle, by any route: an entry parameter, a foreign-call result, or a field or element of a struct that the host fills | The host. No ownership moves (§142 rule 1). | Copies it, keeps it in any object, closure, or module global, and uses it in a later call. It cannot free the host object. |
| A scalar, or a struct with the C layout | The script, as a copy | Reads a copy by value, as a `@ValueType` value, with no Context allocation. A completion copies the C bytes (§178 rule 7). A `V \| null` struct is a box, not a by-value copy (§124 rule 1). |
| A `string` or `u8[]` that a completion or the file module delivers | The Context. The completion copies the host bytes into a new value (§184 rule 2, §185 rule 4). | Uses it as any string or array. A collection or the Context release frees it. |
| A `string` field of a struct that the host fills | The Context. The read copies the bytes of the C string view into a new string (§28 rule 3). | Same as the row above. |
| The handle of an async host call (`Promise<T>`) | Counted. The script holders and the host operation each hold a count (§178 rule 2). | Awaits it at least once (§70). It is freed after the last holder releases it and the host completes it (§178 rule 8). |

The host keeps the object of a handle valid while script code of the
Context can use the handle (§142 rule 2). The default is the
Context-scoped lifetime: the object stays valid until the Context and
its pending async work end (§142 rule 3). A host that destroys an
object earlier gives the script a protocol to follow: an explicit
detach point, explicit retain and release calls, or an ID with a
generation that the host validates
([`specs/blocks/examples.md`](../specs/blocks/examples.md) §5b).

Common mistakes, each from the rules above:

- A script object that holds a host handle does not keep the host
  object alive. Only the host decides when the object ends.
- The language and the runtime do not detect a use of a handle after
  the host destroys its object (§142 rule 2).
- A `null` store into one global is not a detach. A copy can remain in
  another object, a closure, or pending async work (`examples.md` §5b).
- A copied `string` or `u8[]` does not change when the host buffer
  changes. The host reads its buffer only during the completion call
  (§184 rule 2).
- A `string` that the script passes to the host is a view of the
  script bytes. It is valid only during the call, so a host that keeps
  it copies it (§28 rule 2).
- A collection does not end a pending host operation. The
  operation roots its waiters until the host completes it (§178 rule
  10).
- A dropped handle does not cancel a host operation. If the host then
  completes it with an `Error` that no `await` observed, the Context
  traps (§178 rule 8).

[Memory is explicit](#memory-is-explicit) has the release table for
every value that the script allocates.

## Strings are UTF-8

A JavaScript string is a sequence of UTF-16 code units. Here a string
is UTF-8 bytes, because that is what a C API takes and returns. Every
index, length, and offset counts bytes:

```ts
export function main(): void {
  const text: string = "café";
  print(`length=${text.length}`);
  print(`slice=${text.slice(0, 3)}`);
  print(`upper=${text.toUpperCase()}`);
}
```

```text
length=5
slice=caf
upper=CAFÉ
```

`"café".length` is 5 here and 4 in Node. `charCodeAt` returns one
UTF-8 byte; `codePointAt` and `charAt` read the code point that starts
at a byte index. Case conversion applies Unicode default case
conversion, without locale rules.

## The standard library is a subset

Arrays (growable `T[]` and `FixedArray<T, N>`), strings, `Map`, `Set`,
`Math`, `Number`, `Date`, typed `JSON`, and regular expressions are
available. Their divergences from JavaScript are documented one by one
in [`generated-docs/api-reference.md`](../generated-docs/api-reference.md),
with the subscript result and the Node result beside each other.

Two properties drive most of the differences.

**Determinism.** `Math.random()` starts from a fixed seed in every
fresh Context, and the host reseeds it, so a run replays. `Date` has
UTC accessors only; local-time accessors are rejected. `Date.now()`
reads the system UTC clock by default, and the host pins it to a
value it chooses. A program that pins the clock and the seed produces
one output for one input.

**A scalar has no miss value.** `T[].find` is absent, because a
missing `i32` has nothing to return; `findIndex` returns `-1`.
`Map.get` returns `V | null` for a reference value, and `Map.getOr`
takes the fallback for a scalar value. `JSON.parse<T>` returns a `T`
against a class you declare, not an `any`, and raises `TypeError` when
the document does not match it.

What is outside the subset is rejected at compile time, with `S014`
and a named replacement. It does not fail at run time.

## Modules

`import` and `export` work between script files. `math.ts` exports a
function, and `main.ts` beside it imports the name:

```ts file=math.ts
export function triangular(n: i32): i32 {
  return (n * (n + 1)) / 2;
}
```

```ts file=main.ts
import { triangular } from "./math";

export function main(): void {
  print(`t(5)=${triangular(5)}`);
}
```

```text
t(5)=15
```

The surface is narrower than TypeScript's: named imports and
namespace imports (`import * as math from "./math"`) from
same-directory siblings (`./name`). A namespace is a static
qualifier: `math.triangular(5)` resolves at compile time, and `math`
alone is not a value. There are no parent paths, no
nested paths, no packages, and no default imports. The CLI follows
the imports from the entry file, so `subscript check main.ts` loads
the whole program.

## Diagnostics

Every rejection carries a stable code. The full text of each, with a
pinned corpus example, is in
[`generated-docs/language-reference.md`](../generated-docs/language-reference.md).

| Code | Rejected |
|---|---|
| S001 | `any` |
| S002 | `eval`, `new Function` |
| S003 | Prototype mutation |
| S004 | Undeclared properties on a nominal type |
| S005 | Structural substitution between nominal types |
| S006 | `extends` on a value class |
| S007 | Bare `number` |
| S008 | A numeric literal that does not fit its context |
| S009 | A synchronous capturing lambda that escapes its defining function or the block of a counted capture; an async arrow that captures a `let`, `var`, parameter, or `this`; a `TaskGroup` outside one `const` local |
| S010 | An exception form outside the decided surface: a `throw` of a value outside the `Error` family, `catch (e: any)` |
| S011 | Unions beyond `Ref \| null`, and unnarrowed access |
| S012 | `undefined` |
| S013 | The `Promise` object surface, and an async handle never awaited |
| S014 | Standard-library use outside the subset, and `f16` arithmetic |
| S016 | A name with no declaration |
| S017 | Two declarations of one name in one namespace |
| S018 | A member the receiver type does not declare |
| S100 | Constructs outside the decided surface |

Four warnings exist. `W001` flags a loop allocation that neither
escapes nor is released. `W002` flags a use after `Context.free`.
`W003` flags fresh callback userdata registered in a loop. `W004`
flags a value copy that a function writes through and never reads.

A consequence of this list, stated plainly: existing npm packages and
most existing TypeScript code will not compile here. The ecosystem is
written against the patterns this list rejects. That is structural.
subscript uses TypeScript's syntax and tooling, not its ecosystem.

## Host-enabled files

The host enables whole-file I/O with `--enable-module node:fs/promises`.
The host installs a file provider before module initialization.
This fragment reads UTF-8 text:

```ts
// enable-module: node:fs/promises
import { readFile } from "node:fs/promises";
async function readMessage(path: string): Promise<string> {
  return await readFile(path, "utf8");
}
```

`readFile(path)` returns `u8[]`. `writeFile(path, data)` accepts a string or `u8[]`.
A missing provider or a file failure reaches `await` as an `Error`.
A Worker Context has no provider, so a file call in a Worker completes with that `Error`.
The host interprets paths and completes requests on the Context owner thread.
C25 records the differences from Node.js.

### Work after a read completes

`then` runs the work when the read completes, and `catch` handles a
failed read. The work can be a named function or a synchronous arrow
([`corpus/accept/a356-file-completion-work.ts`](../corpus/accept/a356-file-completion-work.ts)):

```ts
// excerpt of corpus/accept/a356-file-completion-work.ts
  const first = readFile("a.txt", "utf8").then(show);
  print("after then call");
  await first;

  const totals = new Totals();
  const handles: Promise<void>[] = [];
  handles.push(readFile("pending.txt", "utf8").then((text: string): void => {
    totals.count += 1;
    totals.last = text;
    print(`then ${text}`);
  }));
  await Promise.all(handles);

  const missing = readFile("missing.txt", "utf8").catch((e: Error): string => `caught ${e.message}`);
  print(await missing);

class Loader {
  async load(path: string): Promise<void> {
    const text = await readFile(path, "utf8");
    this.count += 1;
    this.text = `${this.text}${text}`;
    print(`${this.name} loaded ${text}`);
  }
}

  const group = new TaskGroup();
  group.add(loader.load("a.txt"));
  group.add(loader.load("b.txt"));
  await group.join();
```

The test host defers the read of `pending.txt` until its next write
request. It completes the read of `missing.txt` with an `Error`. Its
committed output on both tiers:

```text
// excerpt of corpus/accept/a356-file-completion-work.expected
after then call
show alpha
before release count=0
then late
after release write
totals 1 late
after catch call
caught missing file
group started
group loaded alpha
group loaded beta
group 2 alphabeta
all started
all loaded beta
all loaded alpha
all 2 betaalpha
```

Every handle needs one `await` (S013), so collect the handles in an
array or a `TaskGroup` and await them.
A synchronous `then` or `catch` callback can capture a `const` local
(§186). A capture of a `let`, a `var`, or `this` is S009.
A capturing callback allocates an environment that waits for a
collection, and each handle is counted
([Memory is explicit](#memory-is-explicit)).
To update `this`, use a class method: the async method awaits the read
and then updates the fields.

The completion work runs inside the host's async step, on the Context
owner thread, after the host completes the request. A line that the
script prints after the call and before the `await` comes first.

## Tooling

Accepted programs are valid TypeScript, so `tsc` and tsserver work on
them directly. This repository's own gate runs stock `tsc` over every
corpus program. The CLI adds the semantic layer:

```sh
subscript check file.ts              # errors and warnings, with source context
subscript check file.ts --deny-warnings
subscript run file.ts                # execute on the dev JIT
subscript run file.ts --watch        # re-check and hot-reload on edit
subscript emit file.ts -o out/       # emit the ship-tier C
subscript build --source file.ts     # emit C and link a native binary
subscript bind engine.h              # generate the .d.ts mirror of a C header
subscript link-flags                 # what a host links against
```

## Reading on

- [`examples/README.md`](../examples/README.md) — twelve single-concept
  examples, each with its divergence stated, plus the C-host capstones.
- [`docs/tutorial-c-cpp.md`](tutorial-c-cpp.md) — the same language
  from the host's side, with the embedding walkthrough.
- [`docs/tutorial-rust.md`](tutorial-rust.md) — embedding from a Rust
  host, where the JIT and hot reload live in your process.
- [`generated-docs/api-reference.md`](../generated-docs/api-reference.md)
  — the accepted standard-library surface, and every divergence from
  ECMA with both results shown.
- [`specs/blocks/collisions.md`](../specs/blocks/collisions.md) — the
  decision record for each place subscript diverges from TypeScript.
