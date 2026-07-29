# subscript examples

subscript is a TypeScript-shaped language for logic embedded in a native
application. The host owns the process and main loop; scripts provide typed,
host-callable behavior. These examples show the language differences that
matter when deciding whether that model fits an application.

Read the numbered examples in order:

1. [`e01`–`e04`](#language-foundations) establish sized values, C-layout value
   types, explicit memory management, and nullability.
2. [`e05`–`e08`](#language-foundations) cover failure values, arrays and
   closures, deterministic APIs, and frame-stepped coroutines.
3. [`e09`](e09-c-structs-and-slices.ts) and
   [`e10`](e10-c-callbacks-and-handles.ts) bind the small
   [`engine/`](engine/) C facade.
4. [`host/`](host/) is the capstone: a C `main` owns the loop and calls the
   script's `init`, `update`, and `shutdown` exports.
5. [`context-per-scene/`](context-per-scene/) runs two scenes with fresh
   Contexts, showing script state reset while host frame state continues.

[`gate/two-header-binding.ts`](gate/two-header-binding.ts) is a compiler
phase-proof program, not a teaching example. It binds the `Eng…` facade and
the synthetic `Sub…` [interop fixture](../corpus/interop/) in one program to
prove that two independent header vocabularies coexist.

## Language foundations

Each program has a neighboring `.expected` file containing its exact output.
The rules link to the language's [collision register](../specs/blocks/collisions.md)
or its governing invariant.

| Example | What it teaches | TypeScript divergence | Rule |
|---|---|---|---|
| [`e01-sized-integers`](e01-sized-integers.ts) | `i32`/`u32`/`i64`/`f32`/`f64`, explicit `as` conversions, wrapping | Bare `number` is rejected; literals are contextually typed | C3, C4 |
| [`e02-value-and-reference`](e02-value-and-reference.ts) | `@CStruct class` beside a plain `class`; copy on assign and on pass | Value classes copy; structurally identical declarations are not interchangeable | C2, C1 |
| [`e03-memory`](e03-memory.ts) | Context allocation, `Context.free`, explicit `Context.collect()` | Nothing collects unbidden; never collecting is correct but uses more memory | [Invariant 2](../CLAUDE.md#design-invariants-read-second) |
| [`e04-null`](e04-null.ts) | `T \| null`, narrowing by `!== null` | There is no `undefined` and no general union | C7 |
| [`e05-no-exceptions`](e05-no-exceptions.ts) | Result-shaped returns, `JsonResult` parsing, and traps | There is no `throw` or `try` | C6 |
| [`e06-arrays-and-closures`](e06-arrays-and-closures.ts) | Fixed and growable arrays, bounds checks, `map`/`filter`/`reduce` | Capturing closures may not escape | C5 |
| [`e07-determinism`](e07-determinism.ts) | Seeded `Math.random`, UTC-only `Date`, deterministic number formatting | Locale- and clock-dependent APIs are rejected, not approximated | Q20, Q26 |
| [`e08-coroutines`](e08-coroutines.ts) | A `function*` stepped once per frame | Coroutines replace `async`; there is no event loop | C8 |
| [`e09-c-structs-and-slices`](e09-c-structs-and-slices.ts) | Binding `engine.h`: struct by value, slice, string view, enum, flags | The language struct is the C struct; no marshaling copy changes its layout | [Invariant 1](../CLAUDE.md#design-invariants-read-second) |
| [`e10-c-callbacks-and-handles`](e10-c-callbacks-and-handles.ts) | Opaque-handle lifecycle, callback userdata, deferred pump delivery | Userdata lifetime is explicit; callbacks arrive on the calling thread | Q13, [compiler §14.6](../specs/blocks/compiler.md#146-permanent-non-goal--spontaneous-arbitrary-thread-callbacks) |

## Run the examples

From the repository root:

```sh
cargo test --offline -p subscript-examples
```

This derives the numbered set from the directory, runs every example and the
phase-proof program under both dev-JIT and ship-C-AOT, compares both outputs
byte-for-byte with their committed goldens, regenerates the engine mirror,
and builds and runs both host programs. It needs the repository's Rust
dependencies available offline and the platform C compiler already required
by the ship tier.

To type-check the TypeScript surface with the repository's installed
dependencies:

```sh
npx tsc -p tsconfig.json
```

To build and run either host program directly:

```sh
sh examples/host/build.sh
sh examples/context-per-scene/build.sh
```

Both scripts are thin wrappers over the developer CLI
([`specs/blocks/cli.md`](../specs/blocks/cli.md)): `subscript build --run`
owns the emit → compile → link pipeline, `subscript run <file.ts>` runs a
bindings-free example under the dev JIT, and `subscript link-flags` prints
what a host build must add to link the emitted C.

The host programs need a POSIX shell, the same Rust toolchain and cached
dependencies, and the same platform C compiler; they require no additional
SDK. Releasing a scene Context re-runs `subscript_init` for the next one, so any
state that must span scenes stays host-side.

## How the C binding fits

[`engine/engine.h`](engine/engine.h) is the host-facing C facade, and
[`engine/engine.c`](engine/engine.c) is its deterministic headless
implementation. `subscript-bindgen` generates
[`engine/engine.generated.d.ts`](engine/engine.generated.d.ts); scripts bind
that mirror, while both execution tiers call the declared C functions
directly. The complete host path is
[`host/game.ts`](host/game.ts), [`host/main.c`](host/main.c), and
[`host/build.sh`](host/build.sh); the Context-lifetime counterpart is
[`context-per-scene/`](context-per-scene/).

These examples deliberately contain no device build, benchmark, or trapping
program. Device linkage and performance measurement have their own tooling;
intentional traps live in [`corpus/trap/`](../corpus/trap/). The accept and
reject [corpus](../corpus/) is the executable language definition.
`examples/` is a maintained introduction to that defined behavior.
