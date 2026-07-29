# subscript for TypeScript developers

subscript's syntax is a subset of TypeScript: every accepted program
also type-checks under stock `tsc`, so your editor tooling — tsserver
completion, rename, go-to-definition — works unchanged. The semantics
underneath are not JavaScript's: values have C data layout, integers
have fixed widths, memory is managed explicitly, and the compiler
rejects the dynamic patterns it cannot compile soundly. It is a
scripting language a native application embeds; the application owns
the main loop and calls your exported functions.

This tutorial covers what changes coming from TypeScript. Every
command and output shown was run against the repository as committed.

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

`run` executes under the development tier (a JIT). `print` goes to a
host-owned sink; there is no `console`.

## `number` is gone; integers are sized

JavaScript's `number` is a 64-bit float. Here every numeric type names
its width: `i8/i16/i32/i64`, `u8/u16/u32/u64`, `f32/f64`, and
storage-only `f16`. Using `number` is rejected, and the error names the
alternatives:

```text
error[S007]: bare `number` is rejected; there is no default numeric type — use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)
 --> bare.ts:2:16
  |
2 |   const count: number = 3;
  |                ^
  = rule: Bare `number` is rejected; sized numeric types are mandatory.
error: 1 error(s)
```

Conversions are explicit with `as`, and integer conversions truncate
like a C cast — not like JavaScript's float rounding:

```ts
export function main(): void {
  const wide: i64 = 4000000000;
  const narrow: u8 = 255;
  const truncated: u8 = wide as u8;
  print(`wide=${wide} narrow=${narrow} truncated=${truncated}`);
}
```

```text
wide=4000000000 narrow=255 truncated=0
```

## Types are nominal and closed

Two classes with the same shape are different types; passing one where
the other is expected is rejected (structural substitution is what
makes C-identical layout unverifiable). Objects have exactly their
declared properties — there is no adding properties later, no index
signatures on classes, no prototype mutation.

`@CStruct` marks a class as a **value class**: it has C struct layout
and is copied on assignment and calls, like a struct in C — there is
no aliasing to observe. A plain class is a **reference class**,
heap-allocated with `new`:

```ts
@CStruct
class Vec2 {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }
}

export function main(): void {
  const a: Vec2 = new Vec2(1.0, 2.0);
  const b: Vec2 = a;  // a copy, not a second reference
  b.x = 9.0;
  print(`a.x=${a.x} b.x=${b.x}`);
}
```

```text
a.x=1 b.x=9
```

Value classes cannot use `extends`.

## Memory is explicit — there is no garbage collector

No collector runs on its own, ever. The rules:

- `new` allocates a reference class in the **Context**, the memory
  arena the host application creates and releases.
- `Context.free(x)` releases one allocation immediately.
- `Context.collect()` collects whatever script references can no
  longer reach — but only when you call it.
- A program that never frees anything is **correct**; it retains more
  memory until the host releases the Context. Dropping the last
  reference does not free the object.

The compiler warns where unbounded growth is statically provable:
allocating in a loop without releasing or storing the object is
`warning[W001]`, and using a variable after `Context.free` is
`warning[W002]`:

```text
warning[W001]: `token` is allocated in each loop iteration but neither escapes the iteration nor is released
 --> w01-loop-allocation-unreleased.ts:15:26
   |
15 |     const token: Token = new Token(i);
   |                          ^
   = rule: A reference-class allocation repeated by a loop should escape the iteration or be released.
warning: 1 warning(s)
```

Warnings do not fail the build; `subscript check --deny-warnings`
makes them fail for CI.

## `null` but not `undefined`, and narrowing is mandatory

The only union type is `T | null`, and member access requires
narrowing first — the same control-flow narrowing TypeScript already
taught you, now required:

```ts
class Node {
  value: i32;
  next: Node | null;

  constructor(value: i32, next: Node | null) {
    this.value = value;
    this.next = next;
  }
}

export function main(): void {
  const head: Node = new Node(1, new Node(2, null));
  let cursor: Node | null = head;
  let sum: i32 = 0;
  while (cursor !== null) {
    sum += cursor.value;
    cursor = cursor.next;
  }
  print(`sum=${sum}`);
}
```

```text
sum=3
```

`undefined`, optional properties as absence, and non-null `T | U`
unions are rejected.

## No exceptions; faults trap

`throw` and `try/catch` are not in the language. A runtime fault — an
out-of-range index, integer division by zero, a failed allocation —
records a **trap** in the Context and stops the current entry; the
host reads what happened and where:

```sh
$ subscript run oob.ts   # reads values[5] of a 3-element array
subscript: oob.ts:4:12: trap [index-out-of-bounds]: index 5 out of bounds for array length 3
```

Recoverable conditions are values in the type system (`T | null`),
not exceptions.

## No `async`; coroutines

There is no event loop to schedule promises on — the host application
owns the loop. Suspension is a `function*` coroutine, advanced
explicitly; the host (or your own code) calls `next()` once per step:

```ts
function* updates(): Generator<i32> {
  let position: i32 = 0;
  for (let step: i32 = 1; step <= 3; step += 1) {
    position += step * 2;
    yield position;
  }
}

export function main(): void {
  const update: Generator<i32> = updates();
  for (let frame: i32 = 0; frame < 4; frame += 1) {
    const result = update.next();
    if (result.done) {
      print(`frame=${frame} done`);
    } else {
      print(`frame=${frame} value=${result.value}`);
    }
  }
}
```

```text
frame=0 value=2
frame=1 value=6
frame=2 value=12
frame=3 done
```

## The standard library is a deliberate subset

Arrays, strings, `Map`/`Set`, `Math`, `Date`, `JSON` (typed, via a
declared target class), and regular expressions exist, with documented
divergences from JavaScript where soundness or determinism requires
them. `Math.random()` and `Date.now()` are deterministic: the host
seeds and sets them, so replays reproduce. What is not in the subset
is rejected with `S014` rather than silently missing at runtime.

## What is rejected, and the code that says so

Every rejection carries a stable rule code; these are the ones that
reshape TypeScript habits:

| Code | Rejected |
|---|---|
| S001 | `any` |
| S002 | `eval`, `new Function` |
| S003 | prototype mutation |
| S004 | undeclared properties |
| S005 | structural substitution between nominal types |
| S006 | `extends` on a value class |
| S007 | bare `number` |
| S009 | a capturing lambda escaping its defining function |
| S010 | exceptions |
| S011 | unions beyond `T \| null`, unnarrowed access |
| S012 | `undefined` |
| S013 | `async` / `await` / `Promise` |

A consequence, stated plainly: existing npm packages and most existing
TypeScript code will not compile, because the ecosystem is written
against the dynamic patterns this list rejects. That is structural,
not a missing feature — subscript uses TypeScript's syntax and
tooling, not its ecosystem.

## Tooling you already have

Because accepted programs are valid TypeScript, `tsc` and tsserver
work on them directly — this repository's own gate runs stock `tsc`
over every corpus program. The CLI adds the semantic layer:

```sh
subscript check file.ts       # errors and warnings, with source context
subscript run file.ts         # execute under the dev JIT
subscript emit file.ts -o d/  # emit the ship-tier C
```

## Reading on

- [`examples/README.md`](../examples/README.md) — ten single-concept
  examples, each with its divergence from TypeScript stated, plus two
  complete C-host capstones.
- [`docs/tutorial-c-cpp.md`](tutorial-c-cpp.md) — the same language
  from the host's side, including the embedding walkthrough.
- [`specs/blocks/collisions.md`](../specs/blocks/collisions.md) — the
  decision record for every place subscript diverges from TypeScript.
