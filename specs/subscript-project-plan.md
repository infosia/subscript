# subscript — project plan

**Rev 2 (2026-07-23; Rev 1: 2026-07-22; Rev 0: same day as Rev 1).** This document records the design that founds the
project and the phase plan. It is a working draft, not a contract.
Corrections and revisions land in §8 with evidence; read §8 before
reasoning from anything asserted here.

Provenance: the design decisions recorded here are owner-approved
(2026-07-22). A retired pre-founding proof of concept preceded them;
this repository does not depend on it and does not cite it as evidence —
every claim here stands on evidence produced in this repository
(CLAUDE.md: no external oracle, no sibling references). Claims below
about Perry and Mystral Native are from reading their sources and are
marked *(docs)* where they appear.

## 1. What is being built

A statically-typed, AOT-compilable embedded scripting language for native
host applications that own their main loop and expose a C ABI (game
engines are the archetype, not the boundary — see `CLAUDE.md`):

- **Execution and memory model:** C-compatible data layout; Context-scoped
  memory with manual `delete` and explicitly-invoked collection only; a
  fast-iteration hot-reload development tier (in-process JIT on desktop dev
  platforms) plus an AOT tier for shipping; host-first embedding — the
  host owns the loop and calls exported script functions.
- **Surface syntax:** TypeScript, restricted to a subset that stock `tsc`
  accepts (with an ambient `.d.ts` prelude), so tsserver-based editor
  tooling works unmodified.
- **Deliberately excluded:** npm compatibility and JS semantics. Sound
  typing rejects the unsound patterns the TS ecosystem is written against;
  existing TS code does not carry over, and this is accepted at founding,
  not treated as a gap.

Host interop crosses a C ABI only (invariant 4): the host presents C
headers and the language binds them with zero-copy access (§4). No specific
host header is privileged by the language.

## 2. Why this shape — prior art

| Project | What it proves for this plan | What it does differently |
|---|---|---|
| [AssemblyScript](https://www.assemblyscript.org) | A sound TS-syntax language is buildable and maintainable by a small team; the valid-TS-subset + ambient-types approach gives free editor tooling | Targets WASM only; own tracing GC; host interop crosses linear memory with marshaling — the back half this project replaces |
| [Static TypeScript](https://www.microsoft.com/en-us/research/publication/static-typescript/) (MakeCode) | A TS subset AOT-compiled for ARM microcontrollers — second independent proof of the front-end corner | Education-targeted; nominal classes, restricted dynamism |
| [Perry](https://github.com/PerryTS/perry) | The cautionary bound: AOT over *full* JS/TS semantics spends its budget on runtime guards and ecosystem parity (~40 reimplemented npm packages), and AOT performance is a function of static-proof hit-rate, not a property of AOT (published benchmarks are bimodal, e.g. matrix multiply 66× slower than Node where proofs miss) *(docs)* | Keeps full semantics; NaN-boxed default ABI with guarded typed clones |
| [Mystral Native](https://github.com/mystralengine/mystralnative) | "Compile to native" as runtime-exe + appended JS bundle keeps a JS engine in the frame loop *(docs)* | No AOT; inverse ownership (JS owns the loop) |

The execution/memory-model corner has no row above: it is validated
inside this repository by the standing differential gate (P3) and the
performance gate (P4), not by citation.

## 3. Design invariants

Normative copies live in `CLAUDE.md`; the plan restates them for context.

1. Data layout is **C-ABI-identical (C, not C++)** — platform-ABI-stable,
   compiler-portable, machine-verifiable via `offsetof` assertions.
2. **No implicit GC** — no collector runs unbidden; Context memory, manual
   `delete`, collection is an explicitly invoked host operation.
3. **A fast-iteration development tier and an AOT tier are both
   mandatory.**
4. **Host interop crosses a C ABI only.** Any engine-side data becomes
   script-visible through a C facade authored by the host, never through
   direct C++ binding. Decided at founding to prevent incremental C++
   coupling.
5. **Valid-TS-subset syntax** — `tsc`-clean with the ambient prelude.
6. **Scripts are trusted.**

## 4. C interop patterns

Five patterns recur across real C APIs; the language must express each
natively, with no conversion at the boundary. Language-level primitives
for all five are decided in `specs/blocks/collisions.md`; the binding
vertical slice (P5) exercises them against a neutral synthetic C header.

1. **Intrusive extension chains** — type-safe construction of
   singly-linked lists of heterogeneous structs (a `next` pointer plus a
   type tag in a common embedded header).
2. **`(pointer, count)` array pairs** — the language's slice lowering must
   produce `(ptr, len)` with no conversion; if this fails, the
   zero-copy claim fails with it.
3. **Length-carrying string views** — pointer + length, not
   NUL-terminated; the string representation must expose such a view at
   the boundary.
4. **Callbacks** — C function pointer + `void* userdata`; closures need a
   defined lowering to `(fnptr, userdata)` and a stated userdata lifetime
   rule.
5. **Opaque handles with retain/release** — maps directly onto manual
   lifetime management; with no finalizers in the language there is no
   finalizer-threading problem to solve.

## 5. Semantic collisions

Every collision between TS surface syntax and the language's semantics is
resolved as a written rule with an accept and a reject corpus entry.
The full set (C1–C8, plus the Q-register) is decided in
`specs/blocks/collisions.md`: nominal typing per declaration, `@CStruct`
classes, sized numerics with bare `number` rejected, contextual integer
literals, non-escaping capture only, no exceptions, `T | null` as the only
union, generators-as-coroutines with `async` rejected.

## 6. Phase plan

Contract for P1–P5: `specs/blocks/compiler.md`. Targets: dev Windows/Mac
and `x86_64-unknown-linux-gnu`; ship arm64 iOS/Android plus the desktop
hosts `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and
`x86_64-pc-windows-msvc` (compiler.md §11, §1).

- **P0 — seeding (this revision).** Founding record (this plan,
  `CLAUDE.md`), corpus `a01`–`a24` + `r01`–`r14`, `prelude/lang.d.ts`,
  the `tsc` gate. Exit: `tsc -p tsconfig.json` zero errors; reference
  sweep clean (no external-project references).
- **P0.5 — mobile link spike** *(Rev 1: moved from P3 — §8)*. The spike
  has no dependency on the language; it runs before any compiler
  investment, with the pre-registered kill criterion (compiler block §3).
- **P1 — semantic checker + typed HIR.** All reject entries rejected with
  rule-specific diagnostics at TS positions; accept corpus checks clean.
- **P2 — runtime + JIT.** Runtime crate (Context memory, values, strings,
  arrays, traps, coroutine state, Q14 formatting) + HIR→CLIF lowering +
  `cranelift-jit` execution. Run set (a01–a24) runs; goldens captured per
  the compiler block's procedure.
- **P3 — AOT + hot reload + standing gate.** AOT via
  `cranelift-object`; hot reload demonstrated; the differential gate
  (JIT ≡ AOT ≡ golden, byte-exact) becomes the default `cargo test` path
  and freezes the goldens.
- **P4 — performance gate.** Pre-registered criteria against a
  hand-written C baseline (compiler block §3).
- **P5 — C-header binding vertical slice.** Mirror generator from a
  neutral synthetic C header exercising all five §4 patterns, `offsetof`
  assertion suite, a headless end-to-end slice on both forms, and the
  corpus entries for the five patterns.

Beyond P5 (unscheduled): language surface growth, host scene data through
a C facade, editor debugging depth.

## 7. Standing risks

- **Cranelift ship-tier link** — resolved 2026-07-22: the P0.5 spike
  passed both device triples (`specs/tracking/p0.5-mobile-link.md`); the
  C-emission fallback was not invoked.
- **Single-implementation oracle until P3.** Goldens captured at P2 come
  from one tier; independent confirmation arrives only when P3's AOT path
  reproduces them byte-exactly. Until then a runtime bug can be frozen
  into a golden; the golden-change procedure (compiler block §2) is the
  guard.
- **`tsc`-clean may conflict with future surface growth.** Every new
  spelling must keep the `tsc` gate green (invariant 5); a spelling that
  cannot be made `tsc`-clean is a §8-level correction, not a silent
  compromise.

## 8. Corrections and revisions

Convention: every entry records what changed, the evidence, and the
consequence; later revisions must not reintroduce superseded claims from
memory.

**Rev 1 (2026-07-22) — mobile link spike moved from P3 to P0.5.**

- What changed: Rev 0 scheduled the spike as P3's opening step. It is now
  its own milestone, P0.5, run before P1.
- Evidence: owner decision 2026-07-22. The spike emits a fixed program
  and has no dependency on the checker, runtime, or HIR, so nothing
  orders it after P1/P2; running it first prices the backend bet before
  any compiler investment.
- Consequence: compiler block Rev 1 (§3 criterion unchanged, §4 gains the
  P0.5 row); §7's ship-tier-link risk resolves at P0.5 instead of P3.

**Rev 2 (2026-07-23) — the ship tier is C emission (LLVM), not
`cranelift-object`.**

- What changed: §1's ship tier moves from Cranelift AOT
  (`cranelift-object`) to HIR→C→platform C compiler (`clang -O2`, i.e.
  LLVM). The dev tier stays Cranelift JIT with hot reload, unchanged.
- Evidence: P4 measured Cranelift ship-AOT at 34.4× a hand-written C
  baseline against a 1.5× limit; P4.1 optimized the lowering
  (proof-based bounds-check elimination, copy elision) to 23.2× and a
  profile attributed ≈73% of the residual to Cranelift's scalar,
  unvectorized output from clean branch-free CLIF; P4.2 emitted C from
  the same HIR carrying the same semantics (C2 value copies, checked
  growable arrays) and measured it at **1.05×** — the identical
  computation costs ≈5% through LLVM and ≈23× through Cranelift
  `opt_level=speed`. Full record: `specs/tracking/p4-performance.md`.
- This is not the P0.5 fallback trigger firing. The mobile link spike
  *passed*; C emission was pre-registered (§3) as the fallback for a
  link *failure*, which did not occur. C emission is adopted here for
  *performance*, a separate owner decision (2026-07-23) with the
  measurement as evidence. The pre-registration means the architecture
  (two lowerings, ship = C) was already sanctioned; only the trigger
  differs.
- Consequence: the "one HIR→CLIF lowering serves both tiers; semantics
  coincide by construction" property (§1) holds only for the dev tier;
  the ship tier is a second lowering (HIR→C) whose agreement with the
  dev tier is established **by verification** — the standing gate
  becomes dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact. Implemented as
  P4.3 (compiler block §11): extend the a22-only P4.2 emitter to the
  full run set, rewire the standing gate, and re-verify the device
  triples via `clang` cross-compilation (replacing the `cranelift-object`
  device link; the P0.5/P3 `cranelift-object` path is retained only as
  an optional cross-check, its ship role ended). Invariant 3 (two tiers)
  is unaffected: dev = Cranelift JIT, ship = C/LLVM AOT.
