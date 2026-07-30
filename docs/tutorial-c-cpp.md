# subscript for C and C++ developers

subscript is a statically-typed scripting language for embedding in a
native application. Its syntax is a subset of TypeScript; its execution
and memory model are C-compatible: every language-visible struct has
the layout the platform C ABI gives the equivalent C struct, there is
no garbage collector running behind your code, and the host owns the
main loop. Scripts are trusted first-party logic, not sandboxed
plugins.

This tutorial assumes C, not TypeScript. Every command and output shown
was run against the repository as committed.

## Setup

From the repository root:

```sh
cargo build --release -p subscript-cli
alias subscript=target/release/subscript
```

## The language in five programs

### 1. Hello

```ts
export function main(): void {
  print("hello from subscript");
}
```

```sh
$ subscript run hello.ts
hello from subscript
```

`export function` marks a host-callable entry. Exported entries take no
arguments and return nothing; data crosses through your C API instead
(shown below). `run` executes under the development tier, a JIT — no C
compiler involved.

### 2. Integers are sized, like yours

There is no floating-point-only `number`. The integer types are
`i8/i16/i32/i64`, `u8/u16/u32/u64`; floats are `f32/f64` (`f16` is
storage-only). Conversions are explicit with `as` and truncate the way
a C cast does:

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

(4000000000 is 0xEE6B2800; its low byte is 0.)

### 3. Value structs and heap objects are distinct

`@CStruct` declares a value class: C struct layout, copied on
assignment and on every call, no heap involvement. A plain `class` is a
reference class: `new` allocates it in the Context (the arena your host
owns), and it is freed explicitly.

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

class Particle {
  position: Vec2;

  constructor(position: Vec2) {
    this.position = position;
  }
}

export function main(): void {
  const a: Vec2 = new Vec2(1.0, 2.0);
  const b: Vec2 = a;     // copy, like C struct assignment
  b.x = 9.0;
  print(`a.x=${a.x} b.x=${b.x}`);

  const particle: Particle = new Particle(a);
  print(`particle.x=${particle.position.x}`);
  Context.free(particle);  // explicit, like free()
}
```

```text
a.x=1 b.x=9
particle.x=1
```

### 4. Null is a checked union, not a segfault

The only union type is `T | null`, and the compiler requires narrowing
before member access — the null check you would write anyway, made
mandatory:

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

### 5. Coroutines instead of async

There is no `async`, no promises, no event loop — and that is a
consequence of the embedding model, not a gap: `await`'s semantics
require an event loop that decides when continuations resume, and
your application owns the loop; promise chains also keep captured
environments alive until a collector frees them, and there is no
collector. The suspension mechanism itself stays: a `function*`
coroutine suspends at `yield`, and each `next()` call advances exactly
one step, which matches driving script logic once per frame (the
fuller reasoning is in the
[TypeScript tutorial](tutorial-typescript.md#why-promises-are-absent)):

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

Programs can span files: `import { f } from "./other"` works between
script files, with the usual `export` on the defining side
(`corpus/accept/a19-modules/` is the pinned example). One current
limitation to know: the CLI's `check`/`emit`/`build`/`run` accept a
single source file, so multi-file programs compile through the
repository's `emit-c` tool today, not yet through `subscript`.

## The memory model, in C terms

- A **Context** is an owning arena your host creates and releases.
  Every `new` allocates in it.
- `Context.free(x)` releases one allocation now, like `free`.
- `Context.collect()` is a mark-and-sweep over what script references
  can still reach — but it runs **only when called**. Nothing runs
  behind your back; a program that never collects is correct and
  simply retains more memory until the Context is released.
- Releasing the Context frees everything it owns at once
  (`examples/context-per-scene/` uses one Context per scene for this).
- In the development tier, the runtime can retain and poison freed
  blocks so a stale reference traps instead of reading reused memory —
  an opt-in diagnostics mode
  (`subscript_rt_ctx_set_freed_handle_diagnostics`), bounded by a
  payload threshold and a byte budget.

The compiler also warns statically where growth is provable: an
allocation inside a loop that is neither released nor stored anywhere
is `warning[W001]`; use of a variable after `Context.free` on the same
straight line is `warning[W002]`.

At runtime, the host can watch the same quantities:
`subscript_rt_ctx_live_bytes` / `_live_allocations` /
`_reserved_bytes` report the Context's memory, and
`subscript_rt_ctx_visit_live_allocations` walks every live allocation
with its class and allocation-site ids for attribution
(`specs/blocks/compiler.md` §18.2d, §21.2).

## Embedding subscript in your host, step by step

The finished version of everything below is
[`examples/host/`](../examples/host) — a complete C host over a small
engine facade. This section builds the minimal one.

### Step 1 — the script

```ts
// hello.ts
export function main(): void {
  print("hello from subscript");
}
```

Every exported function becomes a C symbol `subscript_export_<name>`
with the signature `void f(subscript_rt_context*)`. The module also
defines `subscript_init`, which must run once per Context before any
export.

### Step 2 — the host

```c
/* main.c */
#include "subscript_runtime.h"

#include <stdio.h>

int main(void) {
    /* 1. One Context owns every script allocation. */
    subscript_rt_context* ctx = subscript_rt_ctx_new();

    /* 2. Run the module initializer once, then any exported entry. */
    subscript_init(ctx);
    subscript_export_main(ctx);

    /* 3. A trap kind of 0 means the run completed without a fault. */
    if (subscript_rt_ctx_trap_kind(ctx) != 0) {
        uint64_t message_len = 0;
        const uint8_t* message = subscript_rt_ctx_trap_message(ctx, &message_len);
        fprintf(stderr, "script trapped: %.*s\n", (int)message_len, message);
        subscript_rt_ctx_release(ctx);
        return 1;
    }

    /* 4. print() output accumulates in the Context; the host drains it. */
    uint64_t out_len = 0;
    const uint8_t* out = subscript_rt_ctx_stdout(ctx, &out_len);
    fwrite(out, 1, (size_t)out_len, stdout);

    /* 5. Releasing the Context frees every script allocation at once. */
    subscript_rt_ctx_release(ctx);
    return 0;
}
```

There are no exceptions and nothing unwinds across the C boundary: a
script fault (an out-of-range index, integer division by zero, a
failed allocation) records a trap in the Context and returns. The host reads it with
`subscript_rt_ctx_trap_kind` / `_message` / `_pos_id`, or registers a
trap observer to be called at the moment it happens
(`specs/blocks/compiler.md` §18). For streaming `print` output instead
of draining a sink, register a print observer (§18.2f).

### Step 3 — build and run, one command

```sh
$ subscript build --source hello.ts --host main.c -o out --run
hello from subscript
```

`build` emits the script's C, compiles it together with your `main.c`,
links the runtime static library, and (with `--run`) executes the
result. `$CC` overrides the compiler; on Windows without `$CC` it finds
MSVC `cl` itself.

### Step 4 — or integrate with your own build system

Your build owns the final link; subscript hands it two things:

```sh
$ subscript emit hello.ts --no-entry -o gen/
$ subscript link-flags
-I/path/to/runtime/include
/path/to/target/release/libsubscript_runtime.a
```

Compile `gen/program.c` as C11 alongside your sources, add the include
directory, and link the archive. On Windows, `link-flags` also lists
the required system libraries (`kernel32 ntdll userenv ws2_32
dbghelp`). Outside this repository, point the CLI at an installed
runtime with `--runtime-lib`/`--runtime-include` or
`SUBSCRIPT_RUNTIME_LIB`/`SUBSCRIPT_RUNTIME_INCLUDE`.

### Step 5 — expose your engine to scripts

Exported entries take no arguments, so real data crosses through your
own C API: the host presents a C header, and the `bindgen` mirror
generator produces the ambient declarations scripts compile against —
opaque handles, structs (which are your structs, at your offsets:
layout identity is asserted by `offsetof` tests), enums, flags,
callbacks with two userdata slots, and (pointer, count) descriptors as
arrays. See [`examples/engine/engine.h`](../examples/engine/engine.h)
and its generated mirror, the walkthroughs
[`e09-c-structs-and-slices.ts`](../examples/e09-c-structs-and-slices.ts)
and
[`e10-c-callbacks-and-handles.ts`](../examples/e10-c-callbacks-and-handles.ts),
and the capstone [`examples/host/game.ts`](../examples/host/game.ts)
for the whole pattern in use.

### Step 6 — a frame loop: exports beyond `main`

Every `export function <name>(): void` in the script becomes a C
symbol `subscript_export_<name>` with the same signature — in both
tiers, so the host code below is identical whether the script runs
under the dev JIT or as emitted C. The generated header declares only
`subscript_init` and `subscript_export_main`; further exports are
yours to declare, and the shared function-pointer type
`subscript_main_entry` names their signature:

```c
void subscript_export_init(subscript_rt_context *ctx);
void subscript_export_update(subscript_rt_context *ctx);
void subscript_export_shutdown(subscript_rt_context *ctx);
```

The capstone host wraps every call in the same bracket
([`examples/host/main.c`](../examples/host/main.c)):

```c
static bool hostCallScript(
    subscript_rt_context *ctx,
    subscript_main_entry entry) {
    subscript_rt_ctx_enter_script(ctx);
    entry(ctx);
    subscript_rt_ctx_exit_script(ctx);
    return subscript_rt_ctx_trap_kind(ctx) == 0u;
}
```

Four facts make this the whole protocol:

- **`enter`/`exit` bracket each entry.** They maintain the runtime's
  script-depth so trap handling and observers behave correctly
  (`specs/blocks/compiler.md` §18.1a).
- **The return channel is the trap state, nothing else.** Entries
  return `void`; after each call the host asks
  `subscript_rt_ctx_trap_kind` (0 = no fault). The capstone's
  response to a trap is to *detach* — it stops calling script entries
  but lets its own loop finish cleanly, so damaged script state is
  never re-entered.
- **Data is staged, not passed.** Because entries take no arguments,
  the host records the frame's inputs in its own facade before the
  call (`game.ts`'s `update` starts by reading `engFrameWorld()`,
  `engFrameFixedStep()`, `engFrameIndex()`), and the script writes
  results back through the same facade.
- **Script state persists between calls.** Module-level variables
  live in the Context: `game.ts`'s `session`, created in `init`, is
  read and updated by every following `update`. The state's lifetime
  is the Context's, ending at `subscript_rt_ctx_release`.

So a frame loop is: stage inputs → `hostCallScript(ctx,
subscript_export_update)` → read outputs and drain `print` text —
once per frame, with `init` before the first frame and `shutdown`
after the last, exactly as `main.c` does.

### The rules a host must know

Facts that shape a host design, collected in one place; each links its
contract.

- **A Context is single-threaded.** Every `subscript_rt_*` call and
  every export invocation on one Context must come from one thread at
  a time — the header calls this the exclusive Context contract, and
  the callback model is single-threaded under the same scope
  (`specs/blocks/compiler.md` §14.6). Different Contexts on different
  threads are independent.
- **Callbacks reach scripts only on your thread, only when you
  call.** A registered script callback fires on the thread making the
  C call, and never spontaneously from another thread — cross-thread
  delivery is a permanent non-goal (§14.6). If your engine completes
  work on a worker thread, hand the result to the thread that owns
  the Context and deliver it there.
- **Execution is deterministic, and you hold both knobs.**
  `Math.random()` starts from a fixed contract seed in every fresh
  Context — two runs produce the same stream —
  and `subscript_rt_ctx_seed_random` reseeds it. `Date` observes only
  what `subscript_rt_ctx_set_now` sets. Replays and tests reproduce
  byte-for-byte because nothing else feeds either input.
- **A long-running host should stream `print`, not drain it.** The
  Context sink is cumulative and never shrinks through the C API, so
  a script printing every frame grows it without bound. Register a
  print observer (`subscript_rt_ctx_set_print_observer`): each line is
  delivered to your callback and nothing is retained (§18.2f).
- **A trapped Context can detach or recover.** After a trap, the
  capstone's choice is to stop calling script. The alternative is
  `subscript_rt_ctx_clear_trap`, which makes the Context callable
  again when it is safe to (it refuses mid-entry; §18.2b).
- **An entry that never returns is yours to contain.** Calls are
  synchronous and nothing can interrupt one; an accidental infinite
  loop freezes the calling thread, by accepted design — scripts are
  trusted, and isolation against a hung script is the host's to
  supply (Q12). The one bounded subsystem is regular expressions,
  via `subscript_rt_ctx_set_regex_budget`.
- **What you embed is the ship tier.** The runtime has no API for
  loading script source at run time: the emitted C is compiled into
  your binary like any other translation unit. The development tier
  (the JIT behind `subscript run` and the differential gate) serves
  the edit loop; both tiers are held byte-identical on every corpus
  program, which is what makes the split safe.

### Step 7 — the edit loop

`subscript check file.ts` type-checks without building (add
`--mirror engine.generated.d.ts` for programs that bind your header)
and renders errors with source context; `--deny-warnings` turns the
static memory-growth warnings into failures for CI. Because the dev
tier is a JIT and the ship tier is emitted C, both run the same
corpus byte-identically under the repository's standing gate — the
behavior you debug is the behavior you ship.

## Reading on

- [`examples/README.md`](../examples/README.md) — ten single-concept
  examples with expected output, and the two host capstones.
- [`specs/blocks/compiler.md`](../specs/blocks/compiler.md) §18 — the
  host Context C API contract (observers, memory accounting,
  enter/exit).
- [`runtime/include/subscript_runtime.h`](../runtime/include/subscript_runtime.h)
  — the generated header itself; every function is documented.
