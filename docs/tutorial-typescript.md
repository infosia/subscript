# subscript for TypeScript developers

subscript writes the logic that runs inside a native application. The
application owns the process and the main loop. It calls the functions
your script exports. A game engine is the first example, but the same
shape fits audio, simulation, creative tools, and embedded control.

The syntax is a subset of TypeScript. Every accepted program also
type-checks under stock `tsc`, so tsserver gives you completion,
rename, and go-to-definition with no plugin. The semantics below the
syntax are not JavaScript's. Values have C data layout. Integers have
fixed widths. Memory is explicit. The compiler rejects the dynamic
patterns that it cannot compile to predictable machine code.

This page is the list of what changes. Every program and every output
on this page was run against the repository as committed, and every
program type-checks under stock `tsc --strict`.

## The short version

| In TypeScript | In subscript |
|---|---|
| `number` | `i8`–`i64`, `u8`–`u64`, `f16`, `f32`, `f64` |
| Structural types | Nominal types; same shape is not the same type |
| `class` | Reference class (`new`, heap) or `@CStruct` value class (copied) |
| `undefined`, `T \| U` | `Ref \| null` only, narrowed before use |
| `enum` of strings | `type Mode = "fast" \| "safe"`, closed and nominal |
| Garbage collection | `Context.free`, `Context.collect`, `using`; nothing runs unbidden |
| `throw` / `try` | Values (`Ref \| null`) for expected failure, traps for faults |
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

`@CStruct` marks a **value class**. It has C struct layout, and it is
copied on assignment and on every call. Nothing aliases it, so no
second name observes a write. A value class cannot use `extends`.

```ts
@CStruct
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
  while (cursor !== null) {
    if (cursor.value === value) {
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
setting:

- `new` allocates a reference class in the **Context**, the arena the
  host creates and releases.
- `Context.free(value)` releases one allocation at once.
- `Context.collect()` collects what script references no longer reach,
  and it runs only where you write it.
- A program that frees nothing is **correct**. It holds more memory
  until the host releases the Context. Dropping the last reference
  frees nothing by itself.

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

`using` releases at scope exit, in reverse declaration order. The class
declares `[Symbol.dispose]()`, as in TypeScript's explicit resource
management:

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
nor is released. `W002` flags a local read after `Context.free`. `W003`
flags fresh callback userdata registered in a loop. `W004` flags a
value copy that a function writes through and never reads. Warnings do
not fail a build. `subscript check --deny-warnings` makes them fail in
CI.

## Failures are values or traps, never exceptions

`throw`, `try`, and `catch` are not in the language. Two mechanisms
replace them, and the split is deliberate.

An expected failure is a value. A lookup that finds nothing returns
`T | null`. `JSON.parse` returns a `JsonResult<T>` whose `ok` you test
before you read `value`.

A fault is a **trap**. An index outside an array, an integer division
or remainder by zero, a `null` where an `as` narrowing promised a
reference, a use after `Context.free`, and a reached `unreachable()`
are traps. A trap records the rule, the message, and the source
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
suspends the function and every caller up to the entry point. The
computation continues when the host steps it. There is no event loop,
no microtask queue, and no `Promise` object at run time. `Promise<T>`
in an annotation is the `tsc` view of a Context-owned handle.

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

Three forms are awaitable: `Context.suspend()`, a direct call of an
`async` function or `async` instance method, and a handle that an
earlier call produced. A handle lives in a local or an array, and it
passes to another function. Every handle a program creates must have
one awaited completion. `new Promise`, `.then`, `Promise.all`, and the
other statics do not exist.

An async call runs the callee to its first suspension at the call.
A callee that never awaits completes at the call.
An await of a completed handle continues in the same step, where JavaScript yields
([C16](../specs/blocks/collisions.md#c16-an-await-of-a-completed-handle-does-not-yield)).

Three reasons shape this, and each follows from a decision the
repository records:

1. **`await` needs a decision about when work resumes.** In JavaScript
   the event loop decides. Here the host owns the loop, so the host's
   explicit step is the only resumption.
2. **A stored pending promise assumes a collector.** Its continuation
   chain stays alive until something reclaims it. That lifetime has no
   answer without a collector, so a suspended frame stays owned by its
   Context and the Context's release drops it.
3. **Both execution tiers must produce identical bytes.** A microtask
   queue is scheduler state to reproduce exactly in the JIT and in the
   emitted C. Host-stepped suspension has no such state.

A failed `await` returns a value; it does not throw.

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
  if (job === null) {
    return;
  }
  let count: i32 = 0;
  for (let n: i32 = job.from; n < job.to; n += 1) {
    if (n % 2 === 0) {
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
  if (a !== null && b !== null) {
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

There is no top-level program. The application calls what you export.
A one-shot program exports `main`. A frame-driven program exports
entries such as `init`, `update`, and `shutdown`, and the host calls
them.

An export is **host-callable** when three facts hold. It is
synchronous. It returns `void`. Every parameter is a boundary scalar
(a sized numeric or a `boolean`) or an opaque handle from the host's C
API. A host-callable export becomes the C symbol
`subscript_export_<name>`. A zero-argument `void` async export is
host-callable too. Any other export stays a legal script function that
other script code calls; it gets no C symbol.

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
  if (session !== null) {
    session.distance += dt;
  }
}
```

The complete version, where `update` reads the frame's real inputs
from the host's declared API, is
[`examples/host/game.ts`](../examples/host/game.ts). The calling side
is step 6 of the [C/C++ tutorial](tutorial-c-cpp.md).

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
takes the fallback for a scalar value. `JSON.parse<T>` returns a
`JsonResult<T>` against a class you declare, not an `any`.

What is outside the subset is rejected at compile time, with `S014`
and a named replacement. It does not fail at run time.

## Modules

`import` and `export` work between script files. `math.ts` exports a
function, and `main.ts` beside it imports the name:

```ts
export function triangular(n: i32): i32 {
  return (n * (n + 1)) / 2;
}
```

```ts
import { triangular } from "./math";

export function main(): void {
  print(`t(5)=${triangular(5)}`);
}
```

```text
t(5)=15
```

The surface is narrower than TypeScript's: named imports from
same-directory siblings (`./name`). There are no parent paths, no
nested paths, no packages, and no default or namespace imports. The
CLI follows the imports from the entry file, so `subscript check
main.ts` loads the whole program.

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
| S009 | A capturing lambda that escapes its defining function |
| S010 | Exceptions |
| S011 | Unions beyond `Ref \| null`, and unnarrowed access |
| S012 | `undefined` |
| S013 | The `Promise` object surface, and an async handle never awaited |
| S014 | Standard-library use outside the subset, and `f16` arithmetic |
| S016 | A name with no declaration |
| S017 | Two declarations of one name in one namespace |
| S018 | A member the receiver type does not declare |
| S019 | A string literal above the ship-tier byte limit |
| S100 | Constructs outside the decided surface |

Four warnings exist. `W001` flags a loop allocation that neither
escapes nor is released. `W002` flags a use after `Context.free`.
`W003` flags fresh callback userdata registered in a loop. `W004`
flags a value copy that a function writes through and never reads.

A consequence of this list, stated plainly: existing npm packages and
most existing TypeScript code will not compile here. The ecosystem is
written against the patterns this list rejects. That is structural.
subscript uses TypeScript's syntax and tooling, not its ecosystem.

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

- [`examples/README.md`](../examples/README.md) — eleven single-concept
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
