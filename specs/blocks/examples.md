# Examples — contract

Status: Rev 0, 2026-07-28. Contract for `examples/`. The corpus defines
the language; `examples/` teaches it. This document is the contract for
what an example must contain, how the set is verified, and what it may
not do.

## 1. What an example is, and what it is not

An example answers three questions for a reader who knows TypeScript and
does not know this language:

1. **How do I write it?** — ordinary, complete programs, not fragments.
2. **Where does it differ from TypeScript, and why?** — every divergence
   is named at the line where it appears, with the rule that causes it.
3. **How does it meet a C host?** — a host facade, a generated mirror, a
   script that binds it, and a host program that owns the loop.

**An example is not the definition.** `corpus/accept/` and
`corpus/reject/` are (CLAUDE.md core principle 2). An example therefore
**never introduces a language decision**: if an example needs a spelling
or a semantic the corpus does not already cover, the corpus entry is
written first and the example follows it. An example that is the only
place some behaviour is exercised is a defect in the corpus, and is
fixed there.

**An example is not a benchmark and not a device demo.** Every example
runs headless, with no GPU, no window, and no external device (CLAUDE.md
core principle 4), and produces deterministic bytes.

**An example's observable is the value itself, never a hash of it.** A
corpus entry may fold state into a checksum — `a23-game-loop` does — and
the interop fixture must, because a callback's `message.length` is its
only channel. Neither reason transfers: an example prints positions,
counts and flags, which a reader can check by eye and which a golden
discriminates more strongly than a hash. A convention that is right in
`corpus/` is not right here by inheritance; it earns its place against
this section or it does not appear.

*(Written 2026-07-28 after the first draft of the host facade grew an
`engWorldChecksum` for no reason but that the corpus has checksums.)*

## 2. Layout and file conventions

```
examples/
  README.md                        index, build instructions, the divergence table
  e01-<slug>.ts  e01-<slug>.expected
  …
  e10-<slug>.ts  e10-<slug>.expected
  gate/                            phase-proof programs, not teaching material (§2a)
  engine/
    engine.h                       the host's C facade (§4)
    engine.c                       its deterministic implementation
    engine.generated.d.ts          bindgen output — generated, never hand-edited
  host/
    game.ts                        the script side of the capstone (§5)
    main.c                         the host program: owns the loop
    expected.txt                   the capstone's committed output
    build.sh                       desktop build, documented in README.md
  Cargo.toml  build.rs  tests/     the gate (§6)
```

- Filenames: `e<nn>-<slug>.ts`. Numbers are stable identifiers; never
  renumber, as with corpus entries (`corpus.md` §1).
- Every example carries a committed `.expected` holding its exact stdout.
- Every file begins with a header comment block:

```ts
// example: e02-value-and-reference
// teaches: <one line — what a reader takes away>
// differs-from-typescript: <one line, or "nothing">
// see: <corpus entries and spec sections this example follows>
```

- All identifiers, comments and prose are English (CLAUDE.md, Language).
- No path outside the repository appears anywhere, including in
  `build.sh` and `README.md`.

### 2a. `gate/` is not part of the example set

A phase sometimes needs a program that proves a property rather than
teaching one. `compiler.md` §23.7's two-header binding proof is the first:
it must bind the synthetic fixture *and* the facade in one program, and a
reader meeting `SubDevice` beside `EngWorld` learns nothing about their own
header from it.

Such programs live in `examples/gate/`, carry a golden, and run under the
same both-tier comparison as everything else. They are **excluded from the
derived example set**, from `README.md`'s table, and from the `e<nn>`
numbering. A program in `gate/` states in its header comment which
contract clause it proves.

*(Added 2026-07-28: the §23.7 proof was first written as `e10`, which
gave a teaching example a `teaches:` line naming a phase requirement and
put four fixture identifiers in front of a reader.)*

## 3. The comment contract

This is the one thing examples must do that the corpus does not.

1. **Every divergence from TypeScript is commented where it occurs**, and
   the comment names the rule — a `collisions.md` C-number or Q-id, or a
   design invariant. "This is not TypeScript" without the rule is not
   acceptable; the reader must be able to reach the contract.
2. **Comments state the rule and the consequence, not the syntax.**
   `// C2: a @CStruct class is a value — this assignment copies` is a
   comment; `// assign b to a` is noise.
3. **A rejected alternative is shown as a comment, not as code**, with
   the diagnostic the compiler would produce and the reject-corpus entry
   that pins it. Examples must compile; the things that must not compile
   live in `corpus/reject/`.
4. **Density is set by divergence, not by line count.** Code that behaves
   exactly as TypeScript would carries no comment at all.

## 4. The host facade — `examples/engine/`

A small, neutral, game-shaped C API. It is a **second** header alongside
the synthetic interop fixture, and binding it is what `compiler.md` §23
(P25) exists to make possible; that phase lands first.

- **Neutral by construction.** Invented names under an `Eng` / `eng`
  prefix. It names and depends on no external project, library, or
  platform API, exactly as `corpus/interop/interop.h` does
  (`compiler.md` §12.1).
- **Only mappable C.** Structs, enums, pointers, function pointers,
  opaque handles, flag typedefs. No unions, no bitfields.
- **Deterministic and headless.** `engine.c` computes; it does not draw,
  sleep, read a clock, or open anything. The same call sequence produces
  the same bytes on every run and every platform.
- **It carries one instance of each of the five interop patterns**
  (plan §4), because that list is what a reader needs to recognize in
  their own header: an intrusive extension chain, a `(pointer, count)`
  array pair, a length-carrying string view, a callback with userdata,
  and an opaque handle with retain/release.
- **It also carries three shapes the five do not name**, each for its own
  reason, stated here so no declaration is justified by a list that does
  not contain it: a **struct passed by value** with non-trivial padding
  (invariant 1's layout identity is what a reader most needs to see), a
  **flag typedef** whose `static const` members combine with `|`
  (`compiler.md` §13.2), and a **descriptor-embedded count-first array**
  inside a larger struct (§13.2's `<n>Count` / `<n>` recognizer), which is
  how production headers spell an array field.

  *(Corrected 2026-07-28: this section previously presented struct-by-value
  and the flag typedef as plan §4 patterns and omitted the intrusive
  chain, so `engine.h` justified two declarations by citing a list that
  did not require them.)*
- **It carries both slice forms** — a `const` borrow the callee reads and
  a mutable out-array the callee writes (§14.3) — over the **same
  element type**. That pair is what makes the facade discriminating for
  `compiler.md` §23.3: provenance recorded per element type rather than
  per parameter cannot tell the two apart, and this header fails such an
  implementation instead of letting it pass.
- **It exposes the host's frame state as C calls.** Exported script
  functions are zero-argument and `void`
  (`runtime/include/subscript_runtime.h`), so the script does not receive
  `dt` as a parameter: it reads the current world handle and the frame's
  time and index through the facade. That is the idiomatic shape for a
  host-owned loop, and the examples say so rather than working around it.

`engine.generated.d.ts` is produced by this project's `bindgen` from
`engine.h` and is covered by the byte-identical regeneration test
(`compiler.md` §12.2). Hand edits are a defect in the generator
(CLAUDE.md core principle 6).

## 5. The capstone — `examples/host/`

A complete embedding, in C, of the kind invariant 4 describes: the host
owns `main`, owns the loop, and calls exported script functions across
the C ABI.

`main.c` must show, with comments, all of:

1. `sub_rt_ctx_new`, `ss_init`, and release at the end;
2. the frame loop: the host advances its own state, then calls
   `ss_export_update`, bracketed by `sub_rt_ctx_enter_script` /
   `sub_rt_ctx_exit_script` (§18.1a);
3. **trap detection by `sub_rt_ctx_trap_kind`, not by a return value**
   (§18.2c), and the host's choice among the three coherent responses
   (§18.1b) — the example takes one and says why;
4. draining the script's stdout sink with `sub_rt_ctx_stdout`;
5. the memory accounting the host actually gets — `live_bytes` /
   `live_allocations` around an explicit `collect()` (§18.2d), which is
   how "no implicit GC" becomes visible to a host rather than a claim.

**The host prints integers; the script prints floats.** The capstone's
golden must not depend on a libc's float formatting, and it does not have
to: the script's `print` goes through the runtime's own deterministic
number formatting (Q14), which is a property the examples should show
rather than hide. So `main.c` reports counts, flags and frame indices,
and any fractional value is printed from the script side.

`build.sh` builds it on the development desktop with the platform C
compiler: emit the ship-tier C for `game.ts` against the mirror
(`compiler.md` §23.6), compile it with `engine.c` and `main.c`, link the
runtime static library, run it. No NDK and no Xcode device SDK; the
device path already has `codegen/device-link.sh` and is not duplicated
here.

## 6. The example set

Each entry names what it must demonstrate. Output shape is the
implementer's choice; the committed `.expected` freezes it.

| id | teaches | the divergence it names |
|---|---|---|
| `e01-sized-integers` | `i32`/`u32`/`i64`/`f32`/`f64`, explicit `as` conversions, wrapping | C3 bare `number` rejected; C4 literals are contextually typed |
| `e02-value-and-reference` | `@CStruct class` beside a plain `class`; copy on assign and on pass | C2 value types; C1 nominal identity — structurally identical is not interchangeable |
| `e03-memory` | Context allocation, `unsafeDelete`, explicit `collect()` | invariant 2 — nothing collects unbidden; a program that never collects is correct, merely larger |
| `e04-null` | `T \| null`, narrowing by `!== null` | C7 — no `undefined`, no general unions |
| `e05-no-exceptions` | result-shaped returns; `JsonResult` for parsing; what a trap is | C6 — no `throw`, no `try` |
| `e06-arrays-and-closures` | fixed vs growable arrays, bounds checks, `map`/`filter`/`reduce` | C5 — non-escaping capture only |
| `e07-determinism` | seeded `Math.random`, UTC-only `Date`, deterministic number formatting | Q20/Q26 — locale- and clock-dependent APIs are rejected, not silently approximated |
| `e08-coroutines` | `function*` stepped once per frame | C8 — coroutines, not `async` |
| `e09-c-structs-and-slices` | binding `engine.h`: struct by value, slice, string view, enum, flags | zero marshaling — the language struct **is** the C struct (invariant 1) |
| `e10-c-callbacks-and-handles` | opaque handle lifecycle; callback with userdata; a deferred fire the host pumps | Q13 userdata lifetime; §14.6 — callbacks arrive on the calling thread |

`examples/README.md` carries an index, the build and run instructions,
and one table row per example naming the divergence — the reader's
entry point, and the only place the set is summarized.

## 7. The gate (pre-registered)

The examples are verified exactly as the corpus is, for the same reason:
an example that stops compiling and says so is worth having; one that
rots silently teaches a language that no longer exists.

1. **Both tiers, byte-exact, against the committed golden.** Every
   `e<nn>` runs under dev-JIT and ship-C-AOT; the two outputs and the
   `.expected` must agree byte for byte, with no normalization — the
   standing gate's rule (`compiler.md` §2, §8.3).
2. **The set is derived, never enumerated.** The gate reads
   `examples/` and picks up a new example with no edit to test code, as
   `codegen/tests/corpus/mod.rs` does for the corpus.
3. **The capstone is compiled and run**, and its stdout compared with
   `host/expected.txt`. It needs only the platform C compiler, which the
   ship tier already requires, so it is not gated behind a device
   toolchain.
4. **`tsc` clean.** `examples/**/*.ts` and `examples/**/*.d.ts` join
   `tsconfig.json`'s include list; every example type-checks under stock
   `tsc` with the prelude and the mirror (invariant 5).
5. **The mirror regenerates byte-identically** from `engine.h`.
6. The gate is a workspace member (`examples/Cargo.toml`) whose
   `build.rs` compiles `engine/engine.c` into the test binary, so the
   dev-JIT tier has the symbols to register (`compiler.md` §23.5) and the
   ship tier has the sources to link.

**A failing example is a failing build**, on the same `cargo test` path
as everything else. There is no "examples may lag" state.

## 8. Non-goals

- **No device or mobile build.** `codegen/device-link.sh` covers the
  device triples; examples do not duplicate it.
- **No performance claims.** `benchmarks/` is where measurement lives. An
  example never reports a time.
- **No second oracle.** An example's `.expected` is captured from the
  gate like any golden and never becomes the reference for language
  behaviour; the corpus goldens are (`compiler.md` §2).
- **No host framework.** `main.c` is one file that a reader can hold in
  their head. Growing it into a reusable embedding layer would be a
  different deliverable, and would put an untested C library in the path
  of the thing the examples exist to show.
