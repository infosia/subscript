# Corpus — contract

Status: Rev 0, 2026-07-22, seeded at P0 of
`specs/subscript-project-plan.md`. The corpus is the language's executable
definition (CLAUDE.md, core principle 2). This document is the contract
for `corpus/accept/` and `corpus/reject/`; the program files are
maintained against it.

## 1. Layout and file conventions

```
corpus/
  accept/   a01-hello.ts … a24-particle-system.ts
  reject/   r01-any.ts … r14-async.ts
```

- One program per file unless the entry explicitly tests multi-file modules
  (`a19-modules` uses a directory `a19-modules/` with `main.ts` and
  `math.ts`).
- Filenames: `a<nn>-<slug>.ts` (accept), `r<nn>-<slug>.ts` (reject).
  Numbers are stable identifiers; never renumber.
- Every file begins with a header comment block:

```ts
// corpus: accept/a22-matrix-propagation
// purpose: <one line>
// exercises: <comma-separated feature tags>
// questions: <comma-separated Q-ids from §5, or "none">
```

- Reject entries add one line: `// expected-error: <one-line description of
  the diagnostic the compiler must produce>`.
- Multi-file entries: each file's `// corpus:` id includes the filename,
  e.g. `accept/a19-modules/main`.
- **Determinism rule:** every accept program terminates and writes a
  deterministic result via `print(...)`. This is what the standing
  differential gate (plan P3) byte-compares against the goldens. No wall
  clocks, no randomness without a fixed seed, no pointer values in output.
- English only, per CLAUDE.md. No paths outside the repository.
- Golden outputs: `corpus/accept/<id>.expected` (exact stdout bytes),
  introduced per the procedure in `specs/blocks/compiler.md` §2.

## 2. What corpus programs are

Programs are written in the language: a TS subset that type-checks under
stock `tsc` with the ambient prelude (invariant 5). Spellings follow §3;
every row is decided in `specs/blocks/collisions.md`. The `tsc` gate
(`tsc -p tsconfig.json`, zero errors over prelude + accept corpus) is a
standing P0 exit condition and stays green permanently.

The reject corpus is excluded from the `tsc` gate: `r06`–`r14` are
`tsc`-clean by design (they demonstrate rules only this compiler
enforces); `r01`–`r05` reproduce constructs `tsc` may also flag; neither
belongs in the accept gate.

## 3. Ambient spellings

Decided in `specs/blocks/collisions.md` (Q-id resolutions recorded there);
the table stands as the corpus spelling reference.

| Concept | Spelling | Q-id |
|---|---|---|
| Sized numerics | `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f16`, `f32`, `f64` ambient type aliases | Q1, Q23 |
| Bare `number` | Not used anywhere in the corpus; rejected by the compiler | Q1 |
| Value-type struct | `@CStruct class Vec3 { x: f32; y: f32; z: f32 }` — class with ambient `@CStruct` decorator | Q2 |
| Reference class | plain `class` (heap, manual lifetime) | Q2 |
| Fixed-size array field | `FixedArray<f32, 16>` ambient generic | Q3 |
| Slice / `(ptr,len)` | `T[]` parameters lower to `(ptr, len)`; no separate slice type in the surface syntax | Q4 |
| String at boundary | `string` lowers to a length-carrying view (pointer + byte length); no NUL assumption | Q5 |
| Manual delete | prelude function `unsafeDelete(x)` — TS strict mode forbids `delete x` on non-properties, so a delete statement has no TS spelling | Q6 |
| Explicit collection | prelude function `collect()` (host-invoked op exposed to script for the corpus) | Q7 |
| Null story | `T \| null` only; `undefined` never appears in the corpus | Q8 |
| Error handling | return values / result objects; no `throw` in accept corpus | Q9 |
| Closure capture | non-capturing function values freely; capturing lambda appears only in `a14` as the policy probe | Q10 |
| Coroutines | `function*` generators, host-driven via `.next()` | Q11 |
| Entry point | exported `function main(): void` | Q12 |
| Host print | ambient `function print(s: string): void` | Q12 |

## 4. Entry list

### Accept — core semantics

| Id | Purpose |
|---|---|
| a01-hello | Minimal program: `main`, `print`, string literal |
| a02-integer-types | `i32`/`u32`/`f32`/`f64` arithmetic, explicit conversions between them |
| a03-integer-literals | Suffix-less literals flowing into typed contexts (var init, args, array elements) |
| a04-value-struct | `@CStruct class` declaration, field access, copy-on-assign semantics made observable via `print` |
| a05-nominal-identity | Two identically-shaped nominal types used correctly side by side |
| a06-fixed-array | `FixedArray` field inside a value struct (C-layout probe) |
| a07-slice-pair | Function taking `f32[]`, summing it; the `(ptr,len)` lowering probe |
| a08-string-view | String length/slicing at the boundary without NUL assumptions |
| a09-enums | Numeric `enum` declaration and use (C enum lowering probe) |
| a10-control-flow | `if`/`while`/`for`/`switch`/`break`/`continue` |
| a11-functions | Plain functions, default parameters, no overloads |
| a12-generics-mono | One generic function + one generic value struct, two instantiations each |
| a13-closures-noncapture | Function values passed and called, zero capture |
| a14-closures-capture | Single minimal capturing lambda (the Q10 policy probe) |
| a15-manual-lifetime | Reference class: `new`, use, `unsafeDelete` |
| a16-explicit-collect | Allocation, drop of last reference, explicit `collect()` call |
| a17-null-story | `T \| null` parameter and field, narrowing before use |
| a18-error-handling | Fallible operation returning a result value, checked by caller |
| a19-modules | Two-file program: `math.ts` exports, `main.ts` imports |
| a20-coroutine-generator | `function*` yielding values, host loop driving it to completion |
| a21-methods | Methods on a value struct and on a reference class |

### Accept — benchmark and application shapes

| Id | Purpose |
|---|---|
| a22-matrix-propagation | The deciding microbenchmark. Fixed shape so the hand-written C baseline (`specs/blocks/compiler.md` §3) implements the same computation: N=10 000 nodes, arrays of 4×4 `f32` matrices (`local`, `world`) plus a parent-index `i32[]` where `parent[i] < i` (node 0 is the root), world[i] = world[parent[i]] × local[i] propagated in index order, 100 iterations with a deterministic in-place local-matrix perturbation each pass, output = a single `f32` checksum over `world`, seeded by a fixed LCG (no `Math.random`) |
| a23-game-loop | Host-owned loop: exported `init`/`update(dtFixed)`/`shutdown`, fixed dt, entity array of value structs, 60 simulated frames, deterministic state checksum |
| a24-particle-system | Struct-of-arrays vs array-of-structs of value types, tight iteration, deterministic checksum |

### Future — C interop patterns (plan §4, at P5)

The five C interop patterns (plan §4) get accept entries when the P5
binding slice lands, written against the neutral synthetic C header that
slice defines (Q13). Their ids continue the sequence from `a25`; the
numbers are assigned at P5, not reserved here.

### Reject — founding-decided exclusions

| Id | Rejected construct | Expected error |
|---|---|---|
| r01-any | `any` in a declaration | `any` is not part of the language |
| r02-eval | `eval` / `new Function` | no dynamic code evaluation |
| r03-prototype-mutation | assignment through `.prototype` / `Object.setPrototypeOf` | no prototype mutation |
| r04-undeclared-property | writing a property not present in the nominal type | nominal types are closed |
| r05-new-function | `new Function` | no dynamic code evaluation |

### Reject — collision rules (`specs/blocks/collisions.md`)

These are `tsc`-clean by design: they demonstrate rules only this compiler
enforces.

| Id | Rejected construct | Expected error |
|---|---|---|
| r06-structural-substitution | same-shaped class instance where another nominal type is expected | nominal types are not interchangeable |
| r07-value-class-extends | `@CStruct class` with `extends` | value classes do not inherit |
| r08-bare-number | `number` in a declaration | no default numeric type; use a sized type |
| r09-int-literal-overflow | `const x: i32 = 3000000000` | literal out of range for i32 |
| r10-escaping-capture | returning a capturing lambda | capturing lambdas may not escape |
| r11-throw | `throw` statement | exceptions are not in the language |
| r12-general-union | `i32 \| string` field | unions are limited to `T \| null` |
| r13-undefined | `undefined` in annotation/expression | single null story: use `null` |
| r14-async | `async function` | no event loop; use coroutines |

## 5. Question register (Q-ids)

Q-ids are stable identifiers; never renumber. Every row below is resolved
in `specs/blocks/collisions.md` except where marked deferred. New
questions found while writing programs are noted in the file's
`// questions:` line as `Q-new: <text>` and harvested here during review.

- **Q1** — sized-numeric aliases and the `x as T` conversion spelling
  (coupled: branded aliases would make cross-type `as` fail `tsc`).
- **Q2** — value-struct surface syntax; `tsc`-clean-ness of the chosen
  spelling.
- **Q3** — fixed-size arrays: spelling and whether length is type-level.
- **Q4** — slice lowering: `T[]` as the only sequence type vs a distinct
  slice type; ownership at the boundary (plan §4 pattern 2).
- **Q5** — string representation and encoding at the boundary (plan §4
  pattern 3).
- **Q6** — manual-delete spelling; `delete` statement is not available in
  the TS subset.
- **Q7** — how explicit collection is surfaced to script vs host-only.
- **Q8** — single null story: `null` chosen, `undefined` banned — and
  interop with TS optional syntax (`x?: T`).
- **Q9** — exceptions in or out.
- **Q10** — closure capture rule under no-implicit-GC.
- **Q11** — generator-to-coroutine mapping; `async` stays out.
- **Q12** — program entry and minimal host API of the prelude.
- **Q13** — shape of the host C-header ambient mirror (plan §4). Deferred
  to P5: the mirror is generated from the slice's neutral synthetic
  header (generated code never hand-edited — CLAUDE.md); the boundary
  typing rules it relies on are decided in `collisions.md` C7 and Q4/Q5.
- **Q14** — numeric formatting: the corpus prints `f32`/`f64` through
  template literals; tier parity requires a bit-deterministic
  float-to-string rule.
- **Q15** — growable arrays: `T[].push` appears in the benchmark programs;
  growth allocates, so ownership and allocation policy under no-implicit-GC
  must be stated — distinct from the Q4 slice-parameter question.
- **Q16** — host-provided handles: define how corpus programs obtain a
  host-created handle (host-injected entry point vs headless
  implementation). Deferred to P5 with the binding slice.
- **Q17** — mutability through a `const` binding of a value struct: stock
  `tsc` allows field writes through `const` (it blocks rebinding only);
  C rejects writes through a `const` struct; the corpus mutates
  `const`-bound value-struct copies in `a04`, `a22`–`a24`.
- **Q18** — bitwise ops on 64-bit values: C APIs surface `uint64_t` flag
  sets combined with `|`, which has 32-bit semantics in JS; the
  sized-integer operator rules (with Q1) must cover `u64`.

## 6. Review and exit

- Corpus changes land as corpus files only; `specs/` files are edited by
  the planner, not the implementer (CLAUDE.md roles).
- Review checklist: header block present and accurate; determinism rule
  holds; spellings match §3; no constructs outside the decided set
  without a `Q-new` note; reject entries fail for the stated reason, not
  incidentally.
- P0 exit (plan §6): corpus seeded, `tsc` gate zero errors, reference
  sweep clean. Evidence in `specs/tracking/p0-seeding.md`.
