# subscript

A statically-typed, AOT-compilable **embedded scripting language for
native applications**: **TypeScript-subset syntax with zero-copy C
interop**. Structs, arrays, and strings have the layout that C expects,
so script code and its C host read the same bytes, with no conversion
and no copy. Memory is collected automatically, and the host controls
each collection pause.

Because the syntax is a subset of TypeScript, standard TypeScript editor
tooling works against it. The compiler adds sound static types, sized
integers, and value types.

It is built for a host that **owns its main loop and exposes a C ABI**,
and that wants user-authored logic to be fast to iterate on and
predictable at run time.

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
  performance when shipping. A changed function body reaches a running
  program in **0.57 ms**, and a changed whole program in **4.95 ms**; the
  shipping tier needs **119 ms** to check, emit C, compile, and link the
  same program.
- **Native ship performance** — the shipping tier compiles to a native
  binary. On the project's matrix-propagation gate it runs at **1.33× of
  an equivalent hand-written C program** (`clang -O2`, same machine, same
  session); on compute-bound benchmark workloads it reaches **1.00×**. The
  shipping tier emits C and hands it to the platform C compiler
  (LLVM/clang), so it inherits that compiler's optimization; the gap is
  what the language's safety semantics cost over hand-tuned C — a bounds
  check per element access, an overflow and divide check, an allocation
  header.
- **Editor tooling with no custom plugin** — the syntax is a subset of
  TypeScript, so `tsserver` (completion, go-to-definition, inline errors)
  works unmodified against an ambient `.d.ts` prelude. Note that
  `tsserver` checks the permissive TypeScript superset; the language's
  sound rules (integer types, value types, nominal identity) are enforced
  by subscript's own checker, not by the editor.
- **Zero-copy C interop** — bind a C header and call it directly;
  the language's structs *are* the C structs (layout is machine-verified
  against the platform C compiler), so no data is converted or copied at
  the boundary.
- **Collection pauses that the host controls** — a collection runs only
  inside a call into the Context, on the thread of that call, and moves no
  object. The host selects one of three levels: automatic (the default,
  with no release code), host-timed (the host collects at a point it
  selects), or manual (`Context.free`, and no collection).

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
  owns the main loop and calls exported functions. The present standard
  library covers *computation* — numbers, strings, collections, and
  Workers. That is its present scope, not a limit of the language: a
  library that reaches files or the network crosses the C ABI, and its
  async results arrive at a host step.
- **Not a sandbox.** A script is first-party code, and the compiler
  spends its effort on early, precise diagnostics for honest mistakes
  rather than on containing hostile ones. A host that runs content it
  did not write adds an isolation boundary of its own.

## Tutorials

- [subscript for C and C++ developers](docs/tutorial-c-cpp.md) — the
  language from the host's side, ending in a step-by-step embedding
  walkthrough (a complete host is 31 lines of C).
- [subscript for TypeScript developers](docs/tutorial-typescript.md) —
  what changes coming from TypeScript: sized integers, nominal and value
  classes, `null` as the absent value and `undefined` as a function
  result only, the three collection levels, `Error`-family
  exceptions beside uncatchable traps, host-stepped `async`, coroutines,
  workers, and the complete rejection table.
- [subscript for Rust embedders](docs/tutorial-rust.md) — embedding
  through the crates directly: the dev tier in your process, a
  frame-loop host with hot reload in four steps, backed by a
  test-pinned example.

Every command and output shown was run against the repository as
committed.

## What you get

### Sound TypeScript-subset syntax

Every accepted program type-checks under stock `tsc` with the ambient
prelude — that is what makes the TypeScript editor tooling work. The
compiler then *narrows*: `tsc` accepts a superset, and subscript enforces
the sound rules on top (nominal types, sized integers, value types,
restricted unions, `Error`-family exceptions only). A program `tsc` cannot police is
rejected here with a rule-specific diagnostic at the TypeScript source
position.

```ts
@ValueType
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

`@ValueType class` is a C-layout value type (copy-on-assign, copy-on-pass);
`f32`/`i32`/`u32`/`i64`/`u64`/`f64` are sized numerics with C conversion
semantics; a plain `class` is a reference type that the Context allocates.
There is no default `number` type — a sized type is always required.
An `interface` is a compile-time constraint: a class opts in with
`implements`, and a generic `T extends I` calls the members directly in
each instance, so no virtual dispatch and no vtable exist.

### C-ABI-identical data layout

Every language-visible struct lowers to exactly the layout the platform C
ABI gives the equivalent C struct — no vtables, no name mangling. This is
not asserted; it is **machine-verified**: a test compiles a C header with
the platform C compiler and checks the language's computed
`offsetof`/`sizeof`/`_Alignof` against the compiler's own, field by field,
padding included.

### Zero-copy C interop

The host presents C headers; subscript binds them. A generator reads a C
header and emits the ambient `.d.ts` mirror, and the compiler calls the C
functions directly — struct-by-value, `(pointer, count)` array pairs,
length-carrying string views, callbacks with `void* userdata`, and opaque
handles all cross the boundary with no conversion. No specific host header
is privileged by the language; if host data must become script-visible,
the host grows a C facade.

### Collection pauses that the host controls

A collection of a Context runs only inside a call into that Context, on
the thread of that call. No collector thread exists, and no collection
moves an object.

The host selects one of three levels:

| Level | Who decides | What happens | What it is for |
|---|---|---|---|
| Automatic (the default) | The Context | Script code has no release code. The Context reclaims unreachable memory at polls that the compiler places, and at `subscript_rt_ctx_exit_script` when a host call ends. | Most programs |
| Host-timed | The host | The host switches automatic collection off with `subscript_rt_ctx_set_collection(ctx, 0, 0)`. It collects with `subscript_rt_ctx_collect` at a point it selects, for example between two frames. No automatic collection runs. | A frame loop with a fixed collection point |
| Manual | The script | Script code releases an object or an array at once with `Context.free(value)`. Automatic collection is off, and no collection runs. | An audio callback, a control step, or any place that allows no pause |

`Context.free` and `Context.collect()` are valid at each level.

In the ship tier, the mark of an automatic collection runs in steps of
a budget of words, 50,000 by default, and the sweep runs in steps after
the mark, so a large live set or a large heap does not make one long
pause. The host sets the budget with
`subscript_rt_ctx_set_collection_step`. At the host-timed level,
`subscript_rt_ctx_collect_step` runs one pause at a point that the host
selects.

These facts hold at each level:

- No object moves, so a pointer that C holds keeps its address while
  the object lives.
- A collection runs only inside a call into the Context.
- An automatic collection does not change what a program prints or where
  it traps. The two tiers and the interpreter agree byte for byte.
- A program that never collects is correct, merely larger.
- Cleanup of an external resource uses `using`, not reachability.

### A deterministic standard library

The `tsc` side is the ES2022 standard library, so the editor already knows
`Math`, `Date`, `String` and `Array`; subscript accepts a **deterministic
subset** of them with sized-type signatures and rejects the rest with a
clear diagnostic — `tsc` accepts more than the language does, never less.
What is in so far: `Math` (ECMA edge semantics, plus a seeded PRNG so
`Math.random` is replayable), a UTC-only `Date` that erases to `i64`
millis, the `String` and `Array` methods listed in the API reference,
`map`/`filter`/`reduce`/`sort` with real closures, `Map`/`Set`, typed
`JSON` against a class you declare, and regular expressions. Every
operation with a runtime component is implemented **once** and called
through an opaque symbol, so no tier carries its own copy. Every
accepted operation is deterministic given the Context, which is what
makes replay and the golden corpus possible.
An operation whose result depends on a locale table, on a random seed
the program does not control, or on the host's libc is either rejected
or made explicit.

### Two execution tiers, and a third witness

- **Development tier** — an in-process JIT ([Cranelift](https://cranelift.dev))
  with hot reload: a function-body edit is recompiled and swapped at a
  frame boundary; type or layout changes require a restart, and a
  coroutine suspended across a reload is invalidated with a clear trap.
- **Shipping tier** — ahead-of-time compilation to C, built with the
  platform C compiler ([LLVM](https://llvm.org)/clang, or MSVC `cl` on
  Windows) at `-std=c11 -O2`. Ship targets: arm64 devices (iOS,
  Android) and the desktop hosts (macOS arm64, Windows x86-64, Linux
  x86-64). The development tier runs on the same three desktops.

A **reference interpreter** reads the same verified IR. It is written
from the IR contract alone, so it shares no assumption with either
tier.

The tiers are held to **byte-identical output**: a standing
differential gate runs every corpus program under the dev tier and the
ship tier and compares both against a committed golden, on every test
run. The interpreter runs the entries that need no host C library, as
the third witness: 274 of them in the debug profile, and 275 under
`SUBSCRIPT_FULL_INTERPRETER_SWEEP=1` (66 entries declare an
exclusion, 58 of them for a foreign call). The language's behaviour
is defined by that corpus, not by any one backend.

## Performance

Ten workloads, each implemented identically in every language and producing
the same integer checksum (the benchmark refuses to report a workload
unless all subjects agree — same computation, verified). Ratios are to a
hand-written C baseline; **lower is better**, C = 1.00×. One arm64 macOS
machine, one session. Every subject discards warm-up runs until measured
execution passes a 200 ms floor (at least three), then reports the median
of 11 timed runs; a subject whose interquartile range is wider than 15%
of the median is withheld as noise. Full table with absolute times, methodology, and
machine/runtime versions is in [`benchmarks/`](benchmarks/README.md).

| Workload | C | subscript&#8209;ship | subscript&#8209;jit | LuaJIT | JSC | V8 |
|---|---|---|---|---|---|---|
| mandelbrot | 1.00× | **1.00×** | 1.04× | 2.78× | 1.00× | 1.01× |
| fib-recursive | 1.00× | **1.02×** | 2.23× | 1.94× | 1.46× | 2.63× |
| primes | 1.00× | **0.95×** | 1.46× | 2.10× | 0.92× | 1.71× |
| fib-loop | 1.00× | **1.03×** | 2.44× | 1.48× | 1.10× | 1.58× |
| queen | 1.00× | 1.08× | 1.51× | 1.50× | 1.30× | 1.76× |
| sort | 1.00× | 1.16× | 2.25× | 2.28× | 1.48× | 1.83× |
| tree | 1.00× | 1.78× | 7.22× | 2.48× | 0.36× | 0.53× |
| particles | 1.00× | 1.92× | 11.40× | 3.83× | 1.90× | 3.58× |
| collect | 1.00× | 1.15× | 3.64× | 3.79× | 1.13× | 2.78× |
| callbacks | 1.00× | 2.79× | 19.58× | 9.09× | 4.81× | 29.65× |

For what the language looks like at these speeds — twelve commented programs,
a C host facade, and a C host that owns the loop — see
[`examples/`](examples/README.md).

What the numbers show:

- **On compute-bound work the shipping tier is C** — mandelbrot
  1.00×, fib-recursive 1.02×, primes 0.95×, fib-loop
  1.03×, queen 1.08×. The shipping tier *is* the emitted C compiled
  by the same `clang -O2`, and pure-numeric code has almost no array
  traffic to check.
- **The cost is checked memory traffic and value copies** — `sort`
  (bounds-checked growable arrays) at 1.16×, `tree` (per-node allocate and
  free through the Context's size-class arena) at 1.78×, `particles`
  (value-struct arrays) at 1.92×. These are the language's real costs — an
  emitted bounds check per element, value-copy semantics, a 16-byte
  allocation header — not a measurement artifact.
- **`tree` releases each node with `Context.free`.** The mode is the
  default, and no collection runs. One development-tier run counts zero
  automatic collections, because each release subtracts its request from
  the count of requested bytes.
- **`callbacks` (2.79×) is the widest gap**, and it is the idiom's:
  `map`/`filter`/`reduce` over a 1000000-element array 20 times allocates a
  fresh output array per stage, while the C baseline reuses three buffers
  it allocates once. A callback that names a function compiles to a plain
  loop with a direct call; the allocation is what remains. The script has
  no release code. Each round drops two arrays of about 3 to 4 MB, and the
  Context reclaims them automatically: one development-tier run counts seven
  automatic collections. Every runtime pays for the idiom (JSC 4.81×,
  LuaJIT 9.09×, V8 29.65×).
- **`collect` is near C.** It allocates 20000 string-owning nodes per
  round, drops one quarter of them, and reclaims those with an explicit
  `Context.collect()` in each round. One development-tier run counts zero
  automatic collections. The shipping tier reads 1.15× of C, level with JSC
  (1.13×) and ahead of V8 (2.78×) and LuaJIT (3.79×).
- **Against the JITs**, the shipping tier is ahead of LuaJIT on every row.
  It is ahead of JSC on `fib-recursive`, `fib-loop`, `queen`, `sort`, and
  `callbacks`, and level with JSC on `mandelbrot`, `primes`, `particles`, and
  `collect`. It is ahead of V8 on every row except `mandelbrot` (level) and
  `tree`. JSC/V8 lead on `tree`, where garbage-collected bump allocation is
  faster than C.
- **The development tier trades execution speed for iteration speed** —
  the Cranelift JIT is tuned for compile speed and hot reload, not peak
  codegen, and runs 1.04×–19.58×. That is the trade the tier exists to
  make; the next section measures the side it is paid on.

### Iteration speed

Time from a changed source to a running program, on the same
matrix-propagation gate, median of 11 timed runs:

| What changed | Development tier | Shipping tier |
|---|---|---|
| one function body (hot reload) | **0.572 ms** | — |
| the whole program | **4.952 ms** | 118.9 ms (8.7 ms check and emit C, 110.2 ms `cc` compile and link) |

The development tier reaches a running program **24× faster** than the
shipping tier, and a hot reload of one function is **208× faster**. On the
same gate the shipping tier executes at 1.33× of C and the development
tier at 19.86×. All four figures are gated: `specs/blocks/compiler.md` §3
requires the shipping tier within 1.5× of C, either kind of iteration
within 20 ms, and development-tier execution below a 25× ceiling.

This is one benchmark set on one machine; treat the ratios as indicative,
not a leaderboard. The table above is the arm64 / macOS snapshot (the
shipping target), captured 2026-11-21 at 072bfb54. An x86_64 / Windows snapshot
(2026-09-27, at 246c7dd) is in
[`benchmarks/README.windows-x86_64.md`](benchmarks/README.windows-x86_64.md).
It has four subjects, because LuaJIT and JSC are not built there. The runner
withheld one cell as noise (`tree` C), so the `tree` row has no ratios.

## How it works

```
TypeScript-subset source
  → parse (SWC)
  → semantic checker (sound narrowing; rule-specific diagnostics)
  → typed HIR
  → LIR (one ordered IR: evaluation order, control flow, liveness,
    trap sites as data; verified before any consumer reads it)
      ├─ dev tier:  LIR → Cranelift JIT  (hot reload)
      ├─ ship tier: LIR → C → platform C compiler (AOT)
      └─ reference interpreter over LIR (the third witness of the gate)
  all over one runtime: Context memory, values, strings, arrays,
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

To see how each foreign call passes its arguments to C, give `boundary`
the arguments of `check`. It prints one line for each call site and
crossing position:

```sh
subscript boundary game.ts --mirror engine.generated.d.ts
```

For a host with its own build system, emit the C translation unit and
ask what the link line must add:

```sh
subscript bind --header engine.h -o engine.generated.d.ts
subscript emit game.ts --mirror engine.generated.d.ts --no-entry -o out/
subscript link-flags    # runtime include dir, static archive, system libs, removal flag
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
