# subscript

A statically-typed, AOT-compilable **embedded scripting language for
native applications**: a **C-compatible execution and memory model
wearing a TypeScript-subset syntax**. Because the syntax is a subset of
TypeScript, standard TypeScript editor tooling works against it; the
compiler adds sound static types, C-ABI data layout, deterministic
memory, and zero-copy C interop.

It is built for a host that **owns its main loop and exposes a C ABI**,
and that wants user-authored logic to be fast to iterate on and
predictable at run time. Game engines are the archetype and the origin of
the design; the same shape fits real-time audio and DSP plugins, creative
and graphics tools, simulation, and embedded control loops. What the
project measures is the language's properties (see the benchmarks and the
layout proof below); the fit beyond games follows from those properties
rather than from demonstrated adoption in each domain.

subscript is a language project — not a JavaScript runtime, not a
JavaScript binding.

## Why it exists

Embedding a scripting language in a native application usually forces a
choice:

- A **dynamic embedded language** (Lua, JS) — fast iteration and good
  tooling, but boxed values, garbage-collector pauses you do not control,
  and a marshaling layer between the script and the host's C structs.
- **Native C/C++** — no marshaling and full control, but slow iteration
  (recompile-and-relaunch) and no safety net when the code is wrong.

subscript aims for the middle: the **iteration speed and tooling of a
scripting language** with the **data model and performance of C**, and a
sound type system that reports ordinary mistakes as early diagnostics at
the source position.

## Who it's for

Developers writing application, simulation, or tools logic **against a
native, C-ABI host that owns the loop** — gameplay code in an engine, a
DSP block in an audio plugin, a tool script in a content pipeline, a
control step in an embedded system — who want:

- **Fast iteration** — a hot-reload development tier (edit a function
  body, see it swap at the next loop boundary) without giving up native
  performance when shipping.
- **Native ship performance** — the shipping tier compiles to a native
  binary that, on the project's matrix-propagation benchmark, runs within
  **≈5% of an equivalent hand-written C program** (measured 1.05× of
  `clang -O2`). The shipping tier emits C and hands it to the platform C
  compiler (LLVM/clang), so it inherits that compiler's optimization; the
  small gap is the cost of the language's memory-safety semantics over
  hand-tuned C.
- **Editor tooling with no custom plugin** — the syntax is a subset of
  TypeScript, so `tsserver` (completion, go-to-definition, inline errors)
  works unmodified against an ambient `.d.ts` prelude. Note that
  `tsserver` checks the permissive TypeScript superset; the language's
  sound rules (integer types, value types, nominal identity) are enforced
  by subscript's own checker, not by the editor.
- **Zero-marshaling C interop** — bind a C header and call it directly;
  the language's structs *are* the C structs (layout is machine-verified
  against the platform C compiler), so no data is converted or copied at
  the boundary.
- **Deterministic memory** — Context-scoped allocation, explicit
  `Context.free(value)`, and `Context.collect()` only when you ask for it.
  No collector runs unbidden, so there is no pause the host did not ask
  for — the property a frame loop, an audio callback, or a control step
  all need.

### Who it's *not* for

subscript deliberately gives some things up, and these are permanent, not
gaps to be closed later:

- **No npm / existing-TypeScript compatibility.** Sound typing rejects the
  unsound patterns most published TypeScript is written against. Existing
  packages do not carry over.
- **No JavaScript semantics.** No `any`, no prototype mutation, no `eval`,
  no implicit `f64` `number`. The accepted subset is defined by an
  executable corpus, not by JavaScript's spec.
- **Not a standalone program runtime.** subscript is embedded: the host
  owns the main loop and calls exported functions, and platform
  capabilities (files, sockets, devices, threads) come from the host
  through its C ABI rather than from the language. The standard library
  grows in *computation* — numbers, strings, collections — while access to
  the outside world stays the host's to grant. That is a division of
  responsibility, not a capability ceiling.
- **Not a sandbox.** Scripts are first-party, trusted code. The compiler
  spends its effort on early, precise diagnostics for honest mistakes, not
  on containing hostile ones.

## Tutorials

- [subscript for C and C++ developers](docs/tutorial-c-cpp.md) — the
  language from the host's side, ending in a step-by-step embedding
  walkthrough (a complete host is 29 lines of C).
- [subscript for TypeScript developers](docs/tutorial-typescript.md) —
  what changes coming from TypeScript: sized integers, explicit memory,
  value classes, traps, coroutines, and the rejection table.

Every command and output in both was run against the repository as
committed.

## What you get

### Sound TypeScript-subset syntax

Every accepted program type-checks under stock `tsc` with the ambient
prelude — that is what makes the TypeScript editor tooling work. The
compiler then *narrows*: `tsc` accepts a superset, and subscript enforces
the sound rules on top (nominal types, sized integers, value types,
restricted unions, no exceptions). A program `tsc` cannot police is
rejected here with a rule-specific diagnostic at the TypeScript source
position.

```ts
@CStruct
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

`@CStruct class` is a C-layout value type (copy-on-assign, copy-on-pass);
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
is privileged by the language; if host data must become script-visible,
the host grows a C facade.

### No implicit GC

Memory is Context-scoped. Allocate objects normally, release finished
objects with `Context.free(value)`, and call `Context.collect()` when you
want unreachable allocations reclaimed. Nothing collects unbidden — a
program that never collects is correct, merely larger — so there are no
collector pauses in the frame loop.

### A deterministic standard library

The `tsc` side is the ES2022 standard library, so the editor already knows
`Math`, `Date`, `String` and `Array`; subscript accepts a **deterministic
subset** of them with sized-type signatures and rejects the rest with a
clear diagnostic — `tsc` accepts more than the language does, never less.
What is in so far: `Math` (ECMA edge semantics, plus a seeded PRNG so
`Math.random` is replayable), a UTC-only `Date` that erases to `i64`
millis, 17 `String` methods, 16 `Array` methods including
`map`/`filter`/`reduce`/`sort` with real closures, and `Map`/`Set` in
progress. Every operation with a runtime component is implemented **once**
and called by both tiers through an opaque symbol, so the two tiers cannot
drift apart — and every accepted operation is deterministic given the
Context, which is what makes replay and the golden corpus possible.
Anything whose result would depend on a locale table, a random seed the
program cannot control, or the host's libc is either rejected or made
explicit.

### Two execution tiers, checked against each other

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

## Performance

Eight sqrt-free numeric workloads, each implemented identically in every
language and producing the same integer checksum (the benchmark refuses to
report a workload unless all subjects agree — same computation, verified).
Ratios are to a hand-written C baseline; **lower is better**, C = 1.00×.
Apple M2, one machine, median of 21 timed runs after 20 warm-ups. Full
table with absolute times, methodology, and machine/runtime versions is in
[`benchmarks/`](benchmarks/README.md).

| Workload | C | subscript&#8209;ship | subscript&#8209;jit | LuaJIT | JSC | V8 |
|---|---|---|---|---|---|---|
| mandelbrot | 1.00× | **1.00×** | 1.05× | 2.78× | 1.00× | 1.01× |
| queen | 1.00× | **0.99×** | 1.48× | 1.54× | 1.23× | 1.76× |
| primes | 1.00× | **0.96×** | 1.44× | 2.06× | 0.92× | 1.69× |
| fib-recursive | 1.00× | 0.99× | 1.67× | 1.49× | 1.14× | 2.02× |
| fib-loop | 1.00× | 1.02× | 2.01× | 1.50× | 1.09× | 1.58× |
| tree | 1.00× | 1.37× | 10.42× | 2.20× | 0.33× | 0.47× |
| sort | 1.00× | 1.77× | 3.70× | 2.28× | 1.45× | 1.83× |
| particles | 1.00× | 3.07× | 10.35× | 3.84× | 1.90× | 3.58× |

For what the language looks like at these speeds — ten commented programs,
a C host facade, and a C host that owns the loop — see
[`examples/`](examples/README.md).

What the numbers show:

- **On compute-bound work the shipping tier reaches hand-written C** —
  mandelbrot and queen at 1.00×, primes at 0.97×, the fibonacci loops
  within a few percent. The shipping tier *is* the emitted C compiled by
  the same `clang -O2`, and pure-numeric code has no array bounds checks to
  pay for, so it lands on C.
- **The cost is value-copy and checked array traffic** — `particles`
  (value-struct arrays) at 3×, `sort` (bounds-checked growable arrays) at
  1.8×, `tree` (per-node allocate/free through the Context's size-class
  arena) at 1.4×. These are the language's real costs — value-copy
  semantics, an emitted bounds check per element, a 16-byte allocation
  header — not a measurement artifact.
- **Against the JITs**, the shipping tier is at or ahead of LuaJIT on
  every row and level with JSC/V8 on the compute-bound rows; JSC leads on
  `sort` and `particles`, and JSC/V8 lead on `tree`, where
  garbage-collected bump allocation beats even C. The
  development-tier JIT (Cranelift, tuned for compile speed and hot reload,
  not peak codegen) is uniformly slower — the price of the
  fast-iteration tier.

This is one benchmark set on one machine; treat the ratios as indicative,
not a leaderboard. The table above is the arm64 / Apple M2 snapshot (the
shipping target). A x86_64 / Windows snapshot — four subjects, since LuaJIT
and JSC are not built there — is in
[`benchmarks/README.windows-x86_64.md`](benchmarks/README.windows-x86_64.md).
Re-run either yourself with
`cargo run --release -p subscript-benchmarks --bin cross-language`.

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

## Using the `subscript` CLI

The developer command ([`specs/blocks/cli.md`](specs/blocks/cli.md))
owns the emit → compile → link pipeline the examples use. Build it once:

```sh
cargo build --release -p subscript-cli
alias subscript=target/release/subscript   # or put it on PATH
```

Run a program under the development tier (JIT), or type-check it and
produce nothing:

```sh
subscript run examples/e01-sized-integers.ts
subscript check game.ts --mirror engine.generated.d.ts
```

For a host with its own build system, emit the C translation unit and
ask what the link line must add:

```sh
subscript bind --header engine.h -o engine.generated.d.ts
subscript emit game.ts --mirror engine.generated.d.ts --no-entry -o out/
subscript link-flags    # runtime include dir, static archive, system libs
```

Or build and run a complete host in one step — this is all
`examples/host/build.sh` does:

```sh
subscript build \
    --source examples/host/game.ts \
    --mirror examples/engine/engine.generated.d.ts \
    --host examples/engine/engine.c \
    --host examples/host/main.c \
    -o target/examples-host --run
```

Inside this repository the CLI builds and finds the runtime archive
itself. Outside it, point the CLI at an installed runtime with
`--runtime-lib` / `--runtime-include` or the `SUBSCRIPT_RUNTIME_LIB` /
`SUBSCRIPT_RUNTIME_INCLUDE` environment variables.

## Status

The core language is implemented: the semantic checker and typed HIR, the
runtime, both execution tiers, the standing dev≡ship differential gate
over the corpus, a performance gate, the C-header binding slice
(mirror generator, layout proof, and the five interop patterns as corpus
entries), and the `subscript` developer CLI. It is a young language under active development; the surface
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
