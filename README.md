# subscript

A statically-typed, AOT-compilable scripting language for native game
engines: a **C-compatible execution and memory model wearing a
TypeScript-subset syntax**. You write what looks like TypeScript and get
editor tooling for free; the compiler gives you sound static types, C-ABI
data layout, deterministic memory, and zero-marshaling C interop.

subscript is a language project — not a JavaScript runtime, not a
JavaScript binding.

## Why it exists

Game scripting usually forces a choice:

- A **dynamic embedded language** (Lua, JS) — great iteration speed and
  tooling, but boxed values, a garbage collector that pauses when it
  likes, and a marshaling layer between the script and the engine's C
  structs.
- **Native C/C++** — no marshaling and full control, but slow iteration
  (recompile-and-relaunch) and no memory safety net for honest mistakes.

subscript aims for the useful middle: the **iteration speed and tooling of
a scripting language** with the **data model and performance of C**, and a
sound type system that turns honest mistakes into early, precise errors.

## Who it's for

Game and engine developers writing gameplay, simulation, or tools logic
**against a native, C-ABI host**, who want:

- **Fast iteration** — a hot-reload development tier (edit a function
  body, see it swap at the next frame boundary) without giving up native
  performance when shipping.
- **Native ship performance** — the shipping tier compiles to a native
  binary and is measured within **1.5× of hand-written C** on the
  project's matrix-propagation benchmark.
- **Editor tooling with no custom plugin** — the syntax is a subset of
  TypeScript, so `tsserver` (completion, go-to-definition, inline errors)
  works unmodified against an ambient `.d.ts` prelude.
- **Zero-cost C interop** — bind a C header and call it directly; the
  language's structs *are* the C structs (layout is machine-verified
  against the platform C compiler), so there is no marshaling at the
  boundary.
- **Deterministic memory** — Context-scoped allocation, manual `delete`,
  and collection only when you ask for it. No collector runs unbidden, so
  there are no surprise pauses in the frame loop.

### Who it's *not* for

subscript deliberately gives some things up, and these are permanent, not
gaps to be closed later:

- **No npm / existing-TypeScript compatibility.** Sound typing rejects the
  unsound patterns most published TypeScript is written against. Existing
  packages do not carry over.
- **No JavaScript semantics.** No `any`, no prototype mutation, no `eval`,
  no implicit `f64` `number`. The accepted subset is defined by an
  executable corpus, not by JavaScript's spec.
- **Not a general-purpose language.** The design target is game scripting
  against a C-ABI host.

## What you get

### Sound TypeScript-subset syntax

Every accepted program type-checks under stock `tsc` with the ambient
prelude — that is what buys the free editor tooling. The compiler then
*narrows*: `tsc` accepts a superset, and subscript enforces the sound
rules on top (nominal types, sized integers, value types, restricted
unions, no exceptions). A program `tsc` cannot police is rejected here
with a rule-specific diagnostic at the TypeScript source position.

```ts
@value
class Vec3 {
  x: f32;
  y: f32;
  z: f32;
  constructor(x: f32, y: f32, z: f32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}

function add(a: Vec3, b: Vec3): Vec3 {
  return new Vec3(a.x + b.x, a.y + b.y, a.z + b.z);
}

export function main(): void {
  const a: Vec3 = new Vec3(1.0, 2.0, 3.0);
  const b: Vec3 = new Vec3(4.0, 5.0, 6.0);
  const sum: Vec3 = add(a, b);
  print(`${sum.x},${sum.y},${sum.z}`); // 5,7,9
}
```

`@value class` is a C-layout value type (copy-on-assign, copy-on-pass);
`f32`/`i32`/`u32`/`i64`/`u64`/`f64` are sized numerics with C conversion
semantics; a plain `class` is a heap reference type with manual lifetime.
There is no default `number` type — a sized type is always required.

### C-ABI-identical data layout

Every language-visible struct lowers to exactly the layout the platform C
ABI gives the equivalent C struct — no vtables, no name mangling. This is
not asserted; it is **machine-verified**: a test compiles a C header with
the platform C compiler and checks the language's computed
`offsetof`/`sizeof`/`_Alignof` against the compiler's own, field by field,
padding included.

### Zero-marshaling C interop

The host presents C headers; subscript binds them. A generator reads a C
header and emits the ambient `.d.ts` mirror, and the compiler calls the C
functions directly — struct-by-value, `(pointer, count)` array pairs,
length-carrying string views, callbacks with `void* userdata`, and opaque
handles all cross the boundary with no conversion. No specific host header
is privileged by the language; if engine data must become script-visible,
the host grows a C facade.

### No implicit GC

Memory is Context-scoped. You allocate, you `delete` when done, and you
call `collect()` explicitly when you want unreachable allocations
reclaimed. Nothing collects unbidden — a program that never collects is
correct, merely larger — so there are no collector pauses in the frame
loop.

### Two execution tiers, proven equivalent

- **Development tier** — an in-process JIT ([Cranelift](https://cranelift.dev))
  with hot reload: a function-body edit is recompiled and swapped at a
  frame boundary; type or layout changes require a restart, and a
  coroutine suspended across a reload is invalidated with a clear trap.
- **Shipping tier** — ahead-of-time compilation to C, built with the
  platform C compiler ([LLVM](https://llvm.org)/clang) at `-std=c11 -O2`,
  linked for arm64 devices (iOS, Android).

The two tiers are held to **byte-identical output**: a standing
differential gate runs every corpus program under both tiers and compares
the bytes against a committed golden, on every test run. The language's
behaviour is defined by that corpus, not by either backend.

## How it works

```
TypeScript-subset source
  → parse (SWC)
  → semantic checker (sound narrowing; rule-specific diagnostics)
  → typed HIR
      ├─ dev tier:  HIR → Cranelift JIT  (hot reload)
      └─ ship tier: HIR → C → clang/LLVM (arm64 AOT)
  both over one runtime: Context memory, values, strings, arrays,
  traps, coroutine state, deterministic numeric formatting
```

Runtime faults (out-of-bounds, failed narrowing, division by zero) are
**traps**: the Context stops with a diagnostic carrying a source position
and hands control back to the host — no signals, no unwinding across the C
boundary. Coroutines (`function*`) are a CPS transform with suspended
state living in the runtime, so they are safe on platforms without stack
switching.

## The corpus is the definition

The language is defined by an executable corpus, not by prose:

- `corpus/accept/` — programs the language must accept and run, each with
  a committed golden output.
- `corpus/reject/` — programs the compiler must refuse, each with the
  rule it must cite.

A syntax or semantics decision without a corpus entry is not decided. A
sound language is defined as much by what it rejects as by what it
accepts.

## Building and testing

Rust toolchain, plus a C compiler (`cc`/clang) for the shipping tier and
the layout proof.

```sh
cargo test            # checker, runtime, both tiers, the differential gate
```

Editor-tooling / soundness gate (requires Node + TypeScript):

```sh
npm install
npx tsc -p tsconfig.json   # every accept program type-checks under stock tsc
```

Device-triple compile+link check (needs Xcode and/or the Android NDK):

```sh
sh codegen/device-link.sh
```

## Status

The core language is implemented: the semantic checker and typed HIR, the
runtime, both execution tiers, the standing dev≡ship differential gate
over the corpus, a performance gate, and the C-header binding slice
(mirror generator, layout proof, and the five interop patterns as corpus
entries). It is a young language under active development; the surface
grows as the corpus grows.

Design and phase records live in [`specs/`](specs/): `specs/blocks/` holds
the area contracts (corpus, collisions, compiler) and
`specs/subscript-project-plan.md` the overall plan.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in the work by you, as defined in
the Apache-2.0 license, shall be dual-licensed as above, without any
additional terms or conditions.
