# subscript for C and C++ developers

subscript is a statically-typed scripting language for embedding in a
native application. Its syntax is a subset of TypeScript; its execution
and memory model are C-compatible: every language-visible struct has
the layout the platform C ABI gives the equivalent C struct, no garbage
collector runs behind your code, and the host owns the main loop.
Scripts are trusted first-party logic, not sandboxed plugins.

This tutorial assumes C, not TypeScript. Every command and every output
below comes from a run against the repository as committed. The host of
those runs is `aarch64-apple-darwin` with Apple clang 21.

## Setup

From the repository root:

```sh
cargo build --offline --release -p subscript-cli
alias subscript=target/release/subscript
```

The binary has six subcommands. Any other word is a usage error:

```text
$ subscript --help
subscript: unknown subcommand `--help`; usage: subscript <check|emit|bind|link-flags|build|run> ...
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

`export function` marks an entry the host can call. `run` executes the
program under the development tier, a JIT; no C compiler runs. Exports
take parameters — "Step 5" below gives the exact rule and the C
signature.

### 2. Integers are sized, like yours

There is no floating-point-only `number`. The integer types are
`i8/i16/i32/i64`, `u8/u16/u32/u64`; floats are `f32/f64` (`f16` is
storage-only: it declares fields, elements and boundary parameters, and
arithmetic on it is an error). Conversions are explicit with `as` and
truncate the way a C cast does:

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

A value class takes an alignment override, for the case where a device
format demands more than the natural alignment
(`specs/blocks/compiler.md` §62). `N` is 2, 4, 8, or 16, and it must be
at least the natural alignment:

```ts
@CStruct({ align: 16 })
class Vec3f {
  x: f32;
  y: f32;
  z: f32;

  constructor(x: f32, y: f32, z: f32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}
```

The emitted C carries it on the first member, so `sizeof` and
`_Alignof` are the C compiler's answer, not the language's:

```c
typedef struct SubC0 SubC0;
struct SubC0 {
    _Alignas(16) float d0;
    float d1;
    float d2;
};
```

### 4. Null is a checked union, not a segfault

The only union type is `Ref | null`, where `Ref` is a reference class,
an opaque handle, a function type, or a boundary struct pointer. The
compiler requires narrowing before member access. It is the null check
you write anyway, made mandatory:

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

### 5. Async and coroutines, host-stepped

`async`/`await` exist without an event loop: awaiting suspends a
Context-owned frame, and your application resumes pending computations
explicitly — `subscript_rt_ctx_async_step(ctx)` in the frame loop steps
every pending async entry once, in start order, and
`subscript_rt_ctx_async_pending(ctx)` counts them. There are no
`Promise` objects at runtime and promises are not storable values, so
there is nothing to collect and nothing schedules behind your back (the
fuller reasoning is in the
[TypeScript tutorial](tutorial-typescript.md#asyncawait-without-a-scheduler)).
Alongside `await`, generator-shaped suspension is a `function*`
coroutine: each `next()` call advances exactly one step, which matches
driving script logic once per frame:

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
(`corpus/accept/a19-modules/` is the pinned example). The CLI follows
relative imports from the entry file automatically — `subscript check
main.ts` loads the whole program. The decided import surface is
same-directory siblings (`./name`). A parent or nested path is
rejected:

```text
error[S100]: imported module `../math` is not among the program's files
 --> child.ts:1:24
  |
1 | import { addI32 } from "../math";
  |                        ^
```

## The memory model, in C terms

- A **Context** is an owning arena your host creates and releases.
  Every `new` allocates in it.
- `Context.free(x)` releases one allocation now, like `free`.
- `Context.collect()` is a mark-and-sweep over what script references
  can still reach — but it runs **only when called**. Nothing runs
  behind your back; a program that never collects is correct and simply
  retains more memory until the Context is released.
- Releasing the Context frees everything it owns at once
  (`examples/context-per-scene/` uses one Context per scene for this).
- `using x = new T(...)` releases at scope exit, in reverse
  declaration order, for a reference class that declares
  `[Symbol.dispose](): void` (`specs/blocks/compiler.md` §60). The
  hook runs at the natural end of the scope, at `return`, at `break`,
  at `continue`, and at the end of each loop iteration. A trap does
  not run it. Measured on a class whose hook prints and calls
  `Context.free(this)`:

  ```text
  open first
  open second
  work
  close second
  close first
  ```

- In the development tier, the runtime can retain and poison freed
  blocks so a stale reference traps instead of reading reused memory —
  an opt-in diagnostics mode
  (`subscript_rt_ctx_set_freed_handle_diagnostics`), bounded by a
  payload threshold and a byte budget. Call it before the first
  allocation; it returns 1 when it applied and 0 when it did not.
  **Ship-tier behaviour after a use-after-free or a double free is
  deliberately unspecified** (`corpus/trap/t22`, `t23`, their
  `tier-policy` headers). Treat memory-error detection as a
  development-tier facility, and run the dev tier over the same
  program.

The compiler also warns statically where growth or a lost write is
provable. Four codes exist (`compiler/src/warn.rs`): `W001`, an
allocation inside a loop that is neither released nor stored anywhere;
`W002`, use of a variable after `Context.free` on the same straight
line; `W003`, freshly allocated callback userdata registered inside a
loop; `W004`, a value-type copy that is written through and never read.
`--deny-warnings` turns them into a failure, on `check`, `run` and
`build`:

```text
$ subscript check w01.ts --deny-warnings
warning[W001]: `token` is allocated in each loop iteration but neither escapes the iteration nor is released
 --> w01.ts:17:26
   |
17 |     const token: Token = new Token(i);
   |                          ^
   = rule: A reference-class allocation repeated by a loop should escape the iteration or be released.
warning: 1 warning(s)
```

At runtime, the host can watch the same quantities:
`subscript_rt_ctx_live_bytes` / `_live_allocations` / `_reserved_bytes`
report the Context's memory, and
`subscript_rt_ctx_visit_live_allocations` walks every live allocation
with its class and allocation-site ids (`specs/blocks/compiler.md`
§18.2d, §21.2). The count is comparable across the two tiers; the byte
figures are not, because the tiers have different allocators. "Step 6"
below resolves those ids to names and source positions.

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

The module defines `subscript_init`, which must run once per Context
before any export. It defines `subscript_export_main` for the exported
`main`.

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

There are no exceptions and nothing unwinds across the C boundary. A
script fault records a trap in the Context and returns. "Step 6" holds
the trap surface.

### Step 3 — build and run, one command

```sh
$ subscript build --source hello.ts --host main.c -o out --run
hello from subscript
```

`build` emits the script's C, compiles it together with your `main.c`,
links the runtime static library, and (with `--run`) executes the
result. `build` writes the generated standalone `main` only when you
pass no `--host` file, so your `main` is the program's `main`.

The output directory is `-o`. Without `-o`, `build` writes a
`subscript-build` directory beside the source file, never into the
current directory.

The toolchain is clang on a Unix host, and MSVC `cl` on Windows. `$CC`
selects the compiler, and the CLI probes it for `_Float16` support
before it uses it:

```text
$ CC=./notaclang.sh subscript build --source hello.ts -o out --run
subscript: internal lowering error: $CC is set to `./notaclang.sh`, but that compiler cannot compile x86 `_Float16`; set $CC to a capable clang (compiler.md §8.3 and §11a)
```

`build` and `link-flags` find the runtime archive and include directory
in this order: `--runtime-lib` / `--runtime-include`, then
`SUBSCRIPT_RUNTIME_LIB` / `SUBSCRIPT_RUNTIME_INCLUDE`, then the in-repo
default. Outside the repository with neither, both exit 2 and name all
three mechanisms.

### Step 4 — or integrate with your own build system

Your build owns the final link; subscript hands it two things:

```sh
$ subscript emit hello.ts --no-entry -o gen/
$ subscript link-flags
-I/path/to/subscript/runtime/include
/path/to/subscript/target/release/libsubscript_runtime.a
```

The two `link-flags` lines are absolute paths on the host that ran the
command. The first is the include directory that holds the generated
`subscript_runtime.h`. The second is the runtime static archive. This
host is macOS and needs no system library. `link-flags` adds
`kernel32 ntdll userenv ws2_32 dbghelp` on Windows and
`m dl pthread rt util gcc_s c` on Linux (`codegen/src/ship.rs`;
`specs/blocks/compiler.md` §11b). `--cc msvc` prints the include path
as `/I`; the library path is the one this host built.

`emit --no-entry` writes two files into `gen/`:

- `program.c` — the whole program, C11. Compile it as C, with the
  contracted flags `-std=c11 -O2 -fwrapv -ffp-contract=off`
  (`/std:c11 /O2 /fp:strict` under MSVC).
- `program.alloc.h` — the generated tables that turn a `class_id` and a
  `pos_id` into a class name and a TypeScript source position.

Without `--no-entry`, `emit` also writes `entry.c`, a standalone `main`
that runs `subscript_export_main` and drives async exports to
completion. Drop it when your host supplies `main`.

The whole path, run end to end:

```sh
cc -std=c11 -O2 -fwrapv -ffp-contract=off \
   $(subscript link-flags | head -1) \
   gen/program.c main.c \
   $(subscript link-flags | tail -1) \
   -o frame
```

**A C++ host.** `subscript_runtime.h` carries `extern "C"` guards, so a
C++ translation unit includes it directly. Compile `program.c` with the
C compiler, then link its object into the C++ build. A wrapper you
declare yourself needs `extern "C"`:

```c++
extern "C" void subscript_export_step(subscript_rt_context* ctx,
                                      int32_t frame, float dt, int32_t paused);
```

```sh
cc  -std=c11 -O2 -fwrapv -ffp-contract=off $(subscript link-flags | head -1) \
    -c gen/program.c -o gen/program.o
c++ -std=c++17 -O2 $(subscript link-flags | head -1) \
    host.cpp gen/program.o $(subscript link-flags | tail -1) -o cpphost
```

### Step 5 — host-callable exports: the parameters a host passes

An exported function is **host-callable** when three things hold
(`specs/blocks/compiler.md` §59, §61):

1. It is synchronous, or it is `async` with no parameter.
2. It returns `void`.
3. Every parameter is a boundary scalar, an opaque handle, or a
   wire-mapped (`CEnum`) string alias.

For every host-callable export the ship tier emits
`void subscript_export_<name>(subscript_rt_context* ctx, ...)`, with the
same parameter C types the internal function uses. The parameter C
types, measured from the emitted C:

| subscript type | C parameter type |
|---|---|
| `i8` / `u8` | `int8_t` / `uint8_t` |
| `i16` / `u16` | `int16_t` / `uint16_t` |
| `i32` / `u32` | `int32_t` / `uint32_t` |
| `i64` / `u64` | `int64_t` / `uint64_t` |
| `f16` | `uint16_t` (the binary16 bits) |
| `f32` / `f64` | `float` / `double` |
| `boolean` | `int32_t` |
| opaque handle | `void*` |
| wire-mapped alias | `int32_t` |

So this script:

```ts
let total: i32 = 0;

export function step(frame: i32, dt: f32, paused: boolean): void {
  if (paused) {
    print(`frame=${frame} paused`);
    return;
  }
  total += frame;
  print(`frame=${frame} dt=${dt} total=${total}`);
}

export function main(): void {
  step(0, 0.5, false);
}
```

emits this wrapper:

```c
void subscript_export_step(subscript_rt_context* ctx, int32_t a0, float a1, int32_t a2) {
    sub_f0(ctx, a0, a1, a2);
}
```

and the host calls it with the frame's real data:

```c
#include "subscript_runtime.h"

#include <stdint.h>
#include <stdio.h>

/* The generated header declares only subscript_init and
 * subscript_export_main. Declare each further export yourself. */
void subscript_export_step(subscript_rt_context* ctx,
                           int32_t frame, float dt, int32_t paused);

static void hostPrintLine(void* userdata, const uint8_t* line, uint64_t len) {
    (void)userdata;
    fwrite(line, 1, (size_t)len, stdout);
    fputc('\n', stdout);
}

int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (ctx == NULL) {
        return 2;
    }
    subscript_rt_ctx_set_print_observer(ctx, hostPrintLine, NULL);

    subscript_rt_ctx_enter_script(ctx);
    subscript_init(ctx);
    subscript_rt_ctx_exit_script(ctx);

    for (int32_t frame = 1; frame <= 3; frame += 1) {
        subscript_rt_ctx_enter_script(ctx);
        subscript_export_step(ctx, frame, 0.25f, frame == 2);
        subscript_rt_ctx_exit_script(ctx);
        if (subscript_rt_ctx_trap_kind(ctx) != 0u) {
            break;
        }
    }

    subscript_rt_ctx_release(ctx);
    return 0;
}
```

```text
$ subscript build --source frame.ts --host host.c -o out --run
frame=1 dt=0.25 total=1
frame=2 paused
frame=3 dt=0.25 total=4
```

**A parameter is a borrow for the duration of the call.** A handle
value is copyable, so the script can wrap it and store it. The
discipline above the language is yours.

**An export that misses the rule is still a legal script function.** It
gets no C symbol, and in-script callers reach it as before. Two
examples, both measured: `export function label(name: string): void`
and `export function score(): i32` produce no
`subscript_export_` wrapper, and the program still calls them from
`main`. The same holds for a parameter of a value class or a reference
class. An exported `async` function with a parameter is rejected
outright:

```text
error[S100]: exported async function `warm` must have the host entry signature `(): Promise<void>`
```

**A wire-mapped alias parameter validates before the body runs.** A
`CEnum` alias is a string-literal union with a C integer table
(`specs/blocks/compiler.md` §50, §61). The wrapper checks the value the
host passed, and an unmapped value traps with the alias name:

```c
void subscript_export_configure(subscript_rt_context* ctx, int32_t a0, int32_t a1) {
    if (!(a0 == 16 || a0 == 23 || a0 == -7)) { subscript_rt_trap_wire_enum(ctx, (const unsigned char*)"SubWireMode", 11ull, a0, 6u); return; }
    sub_f0(ctx, a0, a1);
}
```

**Zero-argument async exports stay host-callable.** The wrapper starts
the coroutine and returns at the first `await`; your loop then steps it
(Step 8 and "The rules a host must know"):

```c
void subscript_export_warmup(subscript_rt_context* ctx) {
    void* frame = sub_f1(ctx);
    if (*(const uint32_t*)ctx != 0u) return;
    subscript_rt_async_kick(ctx, frame, sub_f1_resume);
}
```

### Step 6 — traps: what the host reads, and what survives

A script fault is a **trap**: an out-of-range index, integer division
by zero, a checked `as` that fails, a failed allocation, and about
twenty more (`runtime/src/trap.rs`, `TrapKind`, kinds 1 through 24 at
this commit). A trap stops the entry that raised it. It never unwinds
across the C boundary, and it never raises a signal.

The Context records the **first** trap of a run and ignores later ones.
Three accessors read it after the call returns. Together they are the
`TrapReport` the in-repo runners print:

```c
uint32_t        subscript_rt_ctx_trap_kind(const subscript_rt_context*);
uint32_t        subscript_rt_ctx_trap_pos_id(const subscript_rt_context*);
const uint8_t*  subscript_rt_ctx_trap_message(const subscript_rt_context*, uint64_t* len);
```

`trap_kind` returns 0 when no trap is pending, and `TrapKind` starts at
1. **The return value of an entry cannot report a fault**: an entry
returns `void`, and a trapped non-`void` internal call returns the
zeroed value. Test `trap_kind`, or register a trap observer
(`subscript_rt_ctx_set_trap_observer`) that fires at the moment of the
fault, while your own frame state is still current
(`specs/blocks/compiler.md` §18.2, §18.2c).

**The Context survives the trap, and the host inspects it.** Measured,
on a script whose `readSlot(index)` reads `bank[index]` with a
three-element array:

```c
subscript_rt_ctx_enter_script(ctx);
subscript_export_readSlot(ctx, 7);
subscript_rt_ctx_exit_script(ctx);

if (subscript_rt_ctx_trap_kind(ctx) != 0u) {
    uint64_t len = 0;
    const uint8_t* message = subscript_rt_ctx_trap_message(ctx, &len);
    printf("host: trap kind=%" PRIu32 " pos_id=%" PRIu32 " message=%.*s\n",
           subscript_rt_ctx_trap_kind(ctx),
           subscript_rt_ctx_trap_pos_id(ctx),
           (int)len, message);
    printf("host: live allocations=%" PRIu64 "\n",
           subscript_rt_ctx_live_allocations(ctx));
    printf("host: clear_trap=%" PRId32 "\n",
           subscript_rt_ctx_clear_trap(ctx));
}

subscript_rt_ctx_enter_script(ctx);
subscript_export_readSlot(ctx, 1);
subscript_rt_ctx_exit_script(ctx);
```

```text
host: trap kind=1 pos_id=0 message=index 7 out of bounds for array length 3
host: live allocations=2
host: clear_trap=1
slot=20
```

That output pins three facts:

- Allocations, module state and the stdout sink are all intact after
  the trap. The host reads them.
- `subscript_rt_ctx_clear_trap` returns 1 and the Context is callable
  again. **It clears reporting state and rolls nothing back**: a run
  that trapped mid-`update` leaves script data exactly as it was at the
  fault. It refuses (returns 0) while a script frame is live, which is
  why the host brackets each entry with
  `subscript_rt_ctx_enter_script` / `_exit_script`
  (`specs/blocks/compiler.md` §18.1a, §18.2b).
- **If the host never clears, the next entry does nothing.** The trap
  flag stays set, and generated code returns immediately after every
  fault-capable call. The host keeps running while the script is
  silently dead. Test `trap_kind` after every entry.

**`pos_id` resolves through `program.alloc.h`.** That header declares
`subscript_alloc_positions`, and `pos_id` indexes it. The same tables
name the class of each live allocation. Measured against
`trapdemo.ts`, whose module global is `let bank: i32[] = [10, 20, 30];`
at line 1 and whose faulting read is at line 4:

```c
#include "program.alloc.h"

/* className scans subscript_alloc_classes for class_id; omitted here. */
static void visit(void* userdata, uint32_t class_id, uint32_t pos_id, uint64_t bytes) {
    const subscript_alloc_position_info* p =
        pos_id < subscript_alloc_position_count ? &subscript_alloc_positions[pos_id] : NULL;
    printf("live: class=%s bytes=%" PRIu64 " site=%s:%" PRIu32 ":%" PRIu32 "\n",
           className(class_id), bytes,
           p ? p->file : "?", p ? p->line : 0u, p ? p->column : 0u);
}
```

```text
positions=8 classes=8
live: class=Array bytes=48 site=trapdemo.ts:1:19
live: class=ArrayData bytes=16 site=trapdemo.ts:1:20
trap: kind=1 pos_id=0 site=trapdemo.ts:4:17
```

A host has three coherent answers to a trap, in increasing cost:
accept the damaged state and continue; detach the failing subsystem,
which is what the trap observer's frame-current context is for; or
release the Context and rebuild from `subscript_init`. Only the third
restores consistency. The capstone
([`examples/host/main.c`](../examples/host/main.c)) detaches.

For streaming `print` output instead of draining a sink, register a
print observer (`subscript_rt_ctx_set_print_observer`, §18.2f). Neither
observer receives a Context pointer, and neither must call any
`subscript_rt_*` function taking that Context — that is an aliasing
violation, not a style rule.

### Step 7 — expose your engine to scripts

Handles and scalars cross as export parameters (Step 5). Everything
else — your structs, your arrays, your callbacks — crosses through your
own C API: the host presents a C header, and the mirror generator
produces the ambient declarations scripts compile against.

```sh
subscript bind --header engine.h -o engine.generated.d.ts
```

Verified on the repository's own facade: the command regenerates
[`examples/engine/engine.generated.d.ts`](../examples/engine/engine.generated.d.ts)
byte-identically from
[`examples/engine/engine.h`](../examples/engine/engine.h). This C —

```c
typedef struct EngineWorld_T *EngineWorld;

typedef struct EngineTransform {
    bool engineInheritScale;
    float engineX;
    float engineY;
    float engineRotation;
    uint16_t engineLayer;
} EngineTransform;

void engineWorldSetName(EngineWorld engineWorld, EngineStringView engineName);
void engineWorldSetTransform(EngineWorld engineWorld, uint32_t engineEntityId,
                             EngineTransform engineTransform);
size_t engineWorldReadEntities(EngineWorld engineWorld, EngineEntityStateOut engineStates);
```

— becomes this mirror:

```ts
// excerpt of examples/engine/engine.generated.d.ts
// @subscript-c-header include="engine.h"
interface EngineWorld {
  readonly __sub_handle_EngineWorld: never;
}

declare class EngineTransform {
  engineInheritScale: boolean;
  engineX: f32;
  engineY: f32;
  engineRotation: f32;
  engineLayer: u16;
  constructor(engineInheritScale: boolean, engineX: f32, engineY: f32, engineRotation: f32, engineLayer: u16);
}

declare function engineWorldSetName(engineWorld: EngineWorld, engineName: string): void;
declare function engineWorldSetTransform(engineWorld: EngineWorld, engineEntityId: u32, engineTransform: EngineTransform): void;
declare function engineWorldReadEntities(engineWorld: EngineWorld, engineStates: EngineEntityState[]): u64;
```

An opaque handle is a branded interface. A length-carrying string view
is `string`. A (pointer, count) descriptor is `T[]`. A struct is your
struct, at your offsets: layout identity is asserted by `offsetof`
tests against the platform C compiler, not claimed.

The frontend is libclang, so it parses real C — preprocessor,
attributes, typedefs, nested structs, function-pointer typedefs, enums,
flag typedefs. A construct it cannot map to a boundary type fails loud
and names the construct; no invalid mirror is written. The output is
generated code: regenerate it, never hand-edit it (this repository's
gate rejects drift between `engine.h` and its committed mirror).

Pass the mirror to every subcommand that reads the program:

```sh
subscript check  game.ts --mirror engine.generated.d.ts
subscript emit   game.ts --mirror engine.generated.d.ts --no-entry -o gen/
subscript build  --source game.ts --mirror engine.generated.d.ts \
                 --host engine.c --host main.c -o out --run
```

`subscript run` takes no `--mirror` and rejects the flag with exit 2. A
program that binds a header is built and run through `build`.

See the walkthroughs
[`e09-c-structs-and-slices.ts`](../examples/e09-c-structs-and-slices.ts)
and
[`e10-c-callbacks-and-handles.ts`](../examples/e10-c-callbacks-and-handles.ts),
and the capstone [`examples/host/game.ts`](../examples/host/game.ts)
for the whole pattern in use.

#### Round-tripping script objects through the host

A host can hold references to script-side objects and hand them back
into script code as arguments — through one decided mechanism, the
registered callback with two userdata slots
([`e10-c-callbacks-and-handles.ts`](../examples/e10-c-callbacks-and-handles.ts)):

```ts
class EventLog { hits: i32 = 0; }
const log: EventLog = new EventLog();     // script-side reference class
const sink: EngineEventSink = new EngineEventSink(
  (message, userdata1, userdata2) => {    // non-capturing (C5)
    if (userdata1 !== null) {
      const eventLog = userdata1 as EventLog;  // checked nominal cast
      eventLog.hits = eventLog.hits + 1;
    }
  },
  log,     // crosses to C as an opaque void*
  null,
);
```

On the C side there is nothing runtime-specific to call: the script's
lambda arrives as an ordinary function pointer, the objects as `void*`,
and firing is a plain C call. From
[`examples/engine/engine.c`](../examples/engine/engine.c), with the
null guards and the local copies removed:

```c
/* Registration, called by the script: store, do not fire. */
void engineWorldSetEventSink(EngineWorld engineWorld, EngineEventSink engineSink) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    engineCheckedWorld->engineEventSink.engineCallback = engineSink.engineCallback;
    engineCheckedWorld->engineEventSink.engineUserdata1 = engineSink.engineUserdata1;
    engineCheckedWorld->engineEventSink.engineUserdata2 = engineSink.engineUserdata2;
    engineCheckedWorld->enginePendingEvent = ENGINE_EVENT_WORLD_READY;
    engineCheckedWorld->engineEventPending = engineSink.engineCallback != NULL;
}

/* Later, on the Context's thread — the capstone pumps once per frame,
 * after the update entry returns (examples/host/main.c). */
void engineWorldPump(EngineWorld engineWorld) {
    struct EngineWorld_T *engineCheckedWorld = engineWorldChecked(engineWorld);
    EngineStringView engineMessage;
    engineMessage.engineData = engineCheckedWorld->engineName;
    engineMessage.engineLen = engineCheckedWorld->engineNameLength;
    engineCheckedWorld->engineLastEvent = engineCheckedWorld->enginePendingEvent;
    engineCheckedWorld->engineEventPending = false;    /* clear before the fire */
    engineCheckedWorld->engineEventSink.engineCallback(  /* fires the script lambda */
        engineMessage,
        engineCheckedWorld->engineEventSink.engineUserdata1,  /* the script objects */
        engineCheckedWorld->engineEventSink.engineUserdata2);
}
```

The real `engineWorldPump` copies the callback and both slots into
locals before it fires, so a callback that re-registers does not change
the call in flight.

The pointers come back to the script typed `object | null`; the script
narrows and casts to the concrete class. Registration itself never
fires (`e10` prints `deferred=0,0` before the pump and `ready=1,…`
after), and the capstone fires from its frame loop as a plain call —
outside the `enter`/`exit` bracket that wraps exported entries. Three
rules govern the round trip:

- **Registered userdata survives collection.** `Context.collect` roots
  every live binding's userdata slots, so an object handed to the host
  is never swept while its registration lives — it is released only by
  explicit `Context.free` or by releasing the Context
  (`specs/blocks/compiler.md` §14.4b). One consequence to plan for:
  replacing a registration does not release the old userdata —
  bindings live for the whole Context — so long-lived hosts free
  superseded userdata explicitly. The intended shape is a long-lived
  userdata object re-registered as-is (identical registrations intern
  to one record and cost nothing); registering *freshly allocated*
  userdata repeatedly grows a record per registration, and both halves
  of that mistake are caught — the compiler warns on the loop-visible
  form (`W003`), and a host-set threshold
  (`subscript_rt_ctx_set_binding_count_advisory`) reports real binding
  growth through the diagnostics observer for the per-frame form only
  the host's loop can produce.
- **Explicit `Context.free` of registered userdata is the remaining
  hazard, and it is caught twice.** At fire time, the trampoline
  refuses to enter script with dead userdata — a
  `callback-userdata-freed` trap, exact while freed-handle diagnostics
  retain the record and best-effort otherwise. At free time, an
  optional observer (`subscript_rt_ctx_set_diagnostics_observer`)
  reports the release of still-registered userdata the moment it
  happens; unset, the check costs nothing.
- **Fire on the Context's thread**, like every other entry (§14.6).
- **Userdata is opaque to C.** Script-only classes do not appear in
  your header; data the host must read belongs in mirrored boundary
  types instead.

The callback is how the host reaches script code that is *not* an
export — a lambda the script registered. An export is the other
direction, and it now carries parameters (Step 5).

### Step 8 — a frame loop: exports beyond `main`

Every host-callable export becomes a C symbol
`subscript_export_<name>` — in both tiers, so the host code below is
identical whether the script runs under the dev JIT or as emitted C.
The generated header declares only `subscript_init` and
`subscript_export_main`; further exports are yours to declare. For a
zero-argument export the shared function-pointer type
`subscript_main_entry` names the signature:

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

Four facts describe the protocol:

- **`enter`/`exit` bracket each entry.** They maintain the runtime's
  script-depth, so trap clearing and the observers behave correctly
  (`specs/blocks/compiler.md` §18.1a).
- **The return channel is the trap state, nothing else.** Entries
  return `void`; after each call the host asks
  `subscript_rt_ctx_trap_kind` (0 = no fault).
- **Data crosses as parameters, or through your facade.** A boundary
  scalar, a handle, or a wire-mapped alias goes in the call itself
  (Step 5). Anything wider — a struct, an array, a string — goes
  through your own C API, which the script reads and writes on both
  sides of the entry. `game.ts`'s `update` starts by reading
  `engineFrameWorld()`, `engineFrameFixedStep()` and
  `engineFrameIndex()`, because that host stages its frame state. A
  host that pushes the same values instead declares
  `export function update(world: EngineWorld, dt: f32): void` and
  passes them. Both shapes are measured, and both work.
- **Script state persists between calls.** Module-level variables live
  in the Context: `game.ts`'s `session`, created in `init`, is read and
  updated by every following `update`. The state's lifetime is the
  Context's, and it ends at `subscript_rt_ctx_release`.

So a frame loop is: stage or gather the frame's inputs →
`hostCallScript(ctx, subscript_export_update)` (or a direct call with
parameters) → read outputs and drain `print` text — once per frame,
with `init` before the first frame and `shutdown` after the last,
exactly as `main.c` does. The capstone's measured run:

```text
host:init index=1
script:init step=0.25
host:state entities=1 flags=3 layer=1
host:frame=0 index=2
script:update x=0.25,step=0.25
host:state entities=1 flags=3 layer=2
...
host:shutdown allocations-before=30 bytes-before=1664
script:shutdown
host:shutdown allocations-after=5 bytes-after=176
```

The last two lines are invariant 2 seen from outside: `shutdown` drops
the last script root and calls `Context.collect()`, and the host
watches 30 live allocations become 5.

### Step 9 — workers, and which thread drives which Context

A **Context is single-threaded**. Every `subscript_rt_*` call and every
export invocation on one Context must come from one thread at a time —
the header calls this the exclusive Context contract
(`specs/blocks/compiler.md` §14.6). That statement is per Context, and
it is not a statement that the language is single-threaded.

Scripts spawn threads. `Worker.spawn(entry)` starts a runtime-owned OS
thread with a **fresh Context of the same program image**, and messages
cross as copies (`specs/blocks/compiler.md` §38–§40, §84;
`specs/blocks/stdlib.md` §16). A worker script looks like this:

```ts
class Job {
  start: i32;
  end: i32;
  constructor(start: i32, end: i32) {
    this.start = start;
    this.end = end;
  }
}

class Total {
  sum: i32;
  constructor(sum: i32) {
    this.sum = sum;
  }
}

function accumulate(inbox: Inbox<Job>, outbox: Outbox<Total>): void {
  const job: Job | null = inbox.wait();   // blocks on the worker's own thread
  if (job === null) {
    return;                               // the parent closed the inbox
  }
  let sum: i32 = 0;
  let value: i32 = job.start;
  while (value < job.end) {
    sum += value;
    value += 1;
  }
  outbox.post(new Total(sum));            // the payload crosses as a byte copy
}

export function main(): void {
  const a: Worker<Job, Total> = Worker.spawn(accumulate);
  const b: Worker<Job, Total> = Worker.spawn(accumulate);
  a.post(new Job(0, 1000));
  b.post(new Job(1000, 2000));
  a.close();
  b.close();
  a.join();                               // traps (kind 22) if the worker trapped
  b.join();
  const first: Total | null = a.poll();   // never blocks
  const second: Total | null = b.poll();
  if (first !== null && second !== null) {
    print(`a=${first.sum} b=${second.sum} total=${first.sum + second.sum}`);
  }
}
```

What the host must know:

- **The host never touches a worker Context.** The runtime creates it
  on the worker thread, runs the module initializer there, runs the
  entry, and releases it there. Thread affinity holds by construction.
- **Your thread still owns your Context.** Nothing in the worker model
  lets a second thread call `subscript_rt_*` on the Context the host
  drives.
- **Worker handles never outlive the parent Context.** Releasing the
  parent closes, joins, and frees every live worker
  (`subscript_rt_worker_spawn` in the generated header).
- **A worker failure is loud at `join`**, as trap kind 22
  (`worker-trapped`) on the joining Context. It is never silent.
- **A worker's `print` goes to the worker's own sink, not to yours.**
  That sink dies with the worker Context. Measured on both tiers: a
  worker that prints produces no line in the parent's output and none
  through the parent's print observer.
- **Messages are copies, including `string` fields** (§84). The
  receiving Context materializes a fresh object and a fresh string per
  slot. A mutation after `post` is not visible on the other side.

Measured, same program, both tiers:

```text
$ subscript run worker.ts
a=499500 b=1499500 total=1999000

$ subscript build --source worker.ts --host workerhost.c -o out --run
host-observer: a=499500 b=1499500 total=1999000
host: trap_kind=0
```

`Worker`, `Inbox` and `Outbox` values are Context-affine: the checker
rejects them as module globals, class fields, array elements, container
type arguments, and lambda captures. That is what keeps a live worker
handle from reaching another Context.
[`examples/e11-parallel-workers.ts`](../examples/e11-parallel-workers.ts)
is the worked example: four workers, four Contexts, one aggregated
result.

### The rules a host must know

Facts that shape a host design, collected in one place; each links its
contract.

- **Callbacks reach scripts only on your thread, only when you call.**
  A registered script callback fires on the thread that makes the C
  call, and never spontaneously from another thread — cross-thread
  delivery is a permanent non-goal (§14.6). If your engine completes
  work on a worker thread, hand the result to the thread that owns the
  Context, and deliver it there.
- **`Math.random` is deterministic; `Date.now` is not.**
  `Math.random()` starts from a fixed contract seed in every fresh
  Context, so two runs produce the same stream, and
  `subscript_rt_ctx_seed_random` reseeds it. `Date.now()` reads the
  system UTC clock by default (`specs/blocks/stdlib.md` §3);
  `subscript_rt_ctx_set_now` pins it. Measured on one program, two
  fresh Contexts in one process:

  ```text
  host: pass=0                       # defaults
  r0=0.7085450778517304
  now=1788833893419
  host: pass=1                       # seed_random(42), set_now(1700000000000)
  r0=0.8143051451229099
  now=1700000000000
  ```

  For a reproducible replay, set both.
- **A long-running host must stream `print`, not drain it.** The
  Context sink is cumulative and a C host cannot drain it, so a script
  that prints every frame grows it without bound. Register a print
  observer (`subscript_rt_ctx_set_print_observer`): each line reaches
  your callback and nothing is retained (§18.2f).
- **An entry that never returns is yours to contain.** Calls are
  synchronous and nothing interrupts one; an accidental infinite loop
  freezes the calling thread, by accepted design — scripts are trusted,
  and isolation against a hung script is the host's to supply. The one
  bounded subsystem is regular expressions, through
  `subscript_rt_ctx_set_regex_budget`.
- **Async scripts complete only if you step them.** An exported `async`
  entry runs to its first `await` and parks; your frame loop calls
  `subscript_rt_ctx_async_step(ctx)` to resume every pending entry once
  (start order, deterministic) and `subscript_rt_ctx_async_pending(ctx)`
  to see whether work remains. A host that never steps leaves them
  parked forever — by design, the same way `Context.collect` never runs
  unbidden. Releasing the Context drops parked computations without
  running any remainder. Measured:

  ```text
  script: warmup: start
  host: frame=0 pending=1
  script: step 1
  host: frame=1 pending=1
  script: step 2
  script: warmup: done after 2
  host: frame=2 pending=0
  ```

- **What you embed is the ship tier.** The runtime has no API for
  loading script source at run time: the emitted C is compiled into
  your binary like any other translation unit. The development tier
  (the JIT behind `subscript run` and `--watch`) serves the edit loop.
  A standing gate holds the two tiers and the committed goldens
  byte-identical on every corpus program (`specs/blocks/compiler.md`
  §8.3). A reference interpreter reads the same IR as a third witness
  over the entries that need no host C library (§85). That gate is
  what makes the split safe.

### Step 10 — the edit loop

`subscript check file.ts` type-checks without building (add
`--mirror engine.generated.d.ts` for programs that bind your header)
and renders errors with source context. A clean check says so:

```text
$ subscript check hello.ts
check: hello.ts: no errors
```

A rejection names the rule, and — where the construct is legal
TypeScript — shows both forms:

```text
$ subscript check bad.ts
error[S007]: bare `number` is rejected; there is no default numeric type — use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)
 --> bad.ts:2:16
  |
2 |   const value: number = 1;
  |                ^
  = rule: Bare `number` is rejected; sized numeric types are mandatory.
  = TypeScript accepts:
  |   const count: number = 3;
  = subscript:
  |   const count: i32 = 3;
  = why: `number` is a 64-bit float with no C width, so every declaration names one of the sized types. (collisions.md C3)
error: 1 error(s)
```

Exit codes are 0 (clean), 1 (program diagnostics), and 2 (usage or
environment). `--deny-warnings` turns the static warnings into a
failure for CI: `check`, `run` and `build` each exit 1 on the `W001`
program above, and each exits 0 without the flag.

For iteration speed, `subscript run --watch file.ts` keeps a program
running while you edit it. A function-body edit is swapped into the
live session — module state survives, which the host relies on — while an
edit that changes a *declaration* (a class field, a signature) is
refused by name, and a broken edit renders its diagnostics while the
old program keeps running. A real session against
[`examples/hot-reload/`](../examples/hot-reload/) (`sh run.sh`, then
edit `demo.ts`). Each column holds one stream in its own captured
order; the rows show the sequence of edits, not a measured
interleaving:

```text
stdout                                       stderr
hot reload: run 1, editable result 10        watch: started
hot reload: run 2, editable result 77        watch: swapped          # body edit
                                             watch: refused: class DemoMarker
hot reload: run 3, editable result 77        watch: swapped          # class restored
hot reload: run 4, editable result 99        watch: swapped          # body edit
                                             error[S100]: type mismatch: the return value expects `i32`, got `string`
                                             watch: waiting for a fix
hot reload: run 5, editable result 55        watch: swapped          # fix
```

`run` counts up across every swap: the module state is the Context's,
and a body swap does not touch it. In-place swap needs the JIT, so
watch mode is dev-tier only — the standing gate is what makes iterating
there and shipping C safe.

`subscript run` covers programs without host C bindings. A program that
binds your header goes through `subscript build` (Step 7).

## Reading on

- [`examples/README.md`](../examples/README.md) — eleven single-concept
  examples with expected output, and the two host capstones.
- [`generated-docs/language-reference.md`](../generated-docs/language-reference.md)
  — every rejection rule with its pinned corpus entry.
- [`generated-docs/api-reference.md`](../generated-docs/api-reference.md)
  — the accepted standard-library surface, and the ES members the
  checker rejects.
- [`specs/blocks/compiler.md`](../specs/blocks/compiler.md) §18 — the
  host Context C API contract (observers, memory accounting,
  enter/exit); §59 and §61 — host-callable exports; §38–§40 and §84 —
  workers.
- [`runtime/include/subscript_runtime.h`](../runtime/include/subscript_runtime.h)
  — the generated header itself; every function is documented.
