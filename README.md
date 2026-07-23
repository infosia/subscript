# subscript

A statically-typed, AOT-compilable scripting language for native game
engines: a **C-compatible execution and memory model wearing a
TypeScript-subset syntax**. Because the syntax is a subset of TypeScript,
standard TypeScript editor tooling works against it; the compiler adds
sound static types, C-ABI data layout, deterministic memory, and
zero-marshaling C interop.

subscript is a language project — not a JavaScript runtime, not a
JavaScript binding.

## Why it exists

Game scripting usually forces a choice:

- A **dynamic embedded language** (Lua, JS) — fast iteration and good
  tooling, but boxed values, garbage-collector pauses you do not control,
  and a marshaling layer between the script and the engine's C structs.
- **Native C/C++** — no marshaling and full control, but slow iteration
  (recompile-and-relaunch) and no safety net when the code is wrong.

subscript aims for the middle: the **iteration speed and tooling of a
scripting language** with the **data model and performance of C**, and a
sound type system that reports ordinary mistakes as early diagnostics at
the source position.

## Who it's for

Game and engine developers writing gameplay, simulation, or tools logic
**against a native, C-ABI host**, who want:

- **Fast iteration** — a hot-reload development tier (edit a function
  body, see it swap at the next frame boundary) without giving up native
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
prelude — that is what makes the TypeScript editor tooling work. The
compiler then *narrows*: `tsc` accepts a superset, and subscript enforces
the sound rules on top (nominal types, sized integers, value types,
restricted unions, no exceptions). A program `tsc` cannot police is
rejected here with a rule-specific diagnostic at the TypeScript source
position.

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
| queen | 1.00× | **1.00×** | 1.47× | 1.51× | 1.22× | 1.76× |
| primes | 1.00× | **0.97×** | 1.44× | 2.08× | 0.92× | 1.69× |
| fib-recursive | 1.00× | 1.00× | 2.15× | 1.84× | 1.49× | 2.64× |
| fib-loop | 1.00× | 1.03× | 1.99× | 1.48× | 1.09× | 1.58× |
| sort | 1.00× | 1.78× | 3.73× | 2.30× | 1.45× | 1.84× |
| particles | 1.00× | 3.06× | 10.29× | 3.84× | 1.90× | 3.58× |
| tree | 1.00× | 10.07× | 10.28× | 2.19× | 0.30× | 0.47× |

What the numbers show, honestly:

- **On compute-bound work the shipping tier reaches hand-written C** —
  mandelbrot and queen at 1.00×, primes at 0.97×, the fibonacci loops
  within a few percent. The shipping tier *is* the emitted C compiled by
  the same `clang -O2`, and pure-numeric code has no array bounds checks to
  pay for, so it lands on C.
- **The cost is allocation and checked array traffic** — `tree` (per-node
  allocate/free) at 10×, `particles` (value-struct arrays) at 3×, `sort`
  (bounds-checked growable arrays) at 1.8×. These are the language's real
  costs — manual per-node allocation, value-copy semantics, an emitted
  bounds check per element — not a measurement artifact.
- **Against the JITs**, the shipping tier is at or ahead of LuaJIT, JSC,
  and V8 on the compute-bound rows and behind them where garbage-collected
  bump allocation wins (`tree`, where JSC/V8 beat C itself). The
  development-tier JIT (Cranelift, tuned for compile speed and hot reload,
  not peak codegen) is uniformly slower — the honest price of the
  fast-iteration tier.

This is one benchmark set on one machine; treat the ratios as indicative,
not a leaderboard. Re-run them yourself with
`cargo run --release -p subscript-bench --bin benchmarks`.

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
