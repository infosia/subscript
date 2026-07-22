# Compiler and runtime — contract

Status: Rev 0, 2026-07-22. Contract for the plan's P1–P5 phases
(`specs/subscript-project-plan.md` §6). Evidence lands in
`specs/tracking/<phase>.md`.

## 1. Architecture

```
SWC parse (TS-subset front end, Rust)
  → semantic checker (C1–C8 + Q rules; rule-specific diagnostics, TS positions)
  → typed HIR
      ├─ dev tier: cranelift-jit (Windows/Mac), hot reload
      └─ ship tier: cranelift-object AOT → .o → Xcode / NDK link
                     (aarch64-apple-ios, aarch64-linux-android; arm64-only)
  both call one runtime crate across a C-ABI-stable boundary:
  Context memory (manual delete, explicit collect), values, strings,
  arrays, traps, coroutine state, Q14 numeric formatting
```

- One HIR→CLIF lowering serves both tiers; dev/ship semantics coincide by
  construction.
- **Coroutines**: CPS transform in codegen (iOS-safe; no fibers, no stack
  switching). Suspended state lives in the runtime as Context data.
- **Traps** (OOB, null narrowing, checked `as`, literal-range, failed
  narrowing): explicitly emitted checks calling runtime trap functions —
  no signal/SEH harvesting on any platform. Trap reports carry TS
  positions (the compiler embeds a position table).
- **Hot reload** (dev tier): per-function indirection table; module
  recompiled and swapped at frame boundaries. Reload eligibility is
  conservative: a swap is accepted only when the module's **declaration
  hash** is unchanged — the hash covers every type declaration (value
  and reference class field names/types/order, enum member values,
  `FixedArray` shapes), every module-level variable's name and type, and
  every function signature; only function-body edits reload, anything
  else requires a restart. Suspended coroutines whose function was
  replaced are invalidated: resuming one traps with a
  "stale coroutine after reload" diagnostic. (Both rules are this
  contract's; P3's reload demo must exercise an accepted body edit, a
  rejected layout edit, and a stale-coroutine trap.)
- Cranelift is pinned (exact `wasmtime`-family crate versions in
  `Cargo.lock`); the pin moves deliberately, never as a side effect.

## 2. Oracle: golden corpus outputs

- Scope: the **run set (a01–a24)** gains committed
  `corpus/accept/<id>.expected` files (exact stdout bytes). Future interop
  entries (plan P5) define their goldens when they land.
- Authoring: goldens are derived from the language rules, not copied from
  any external implementation. Entries whose output is fully determined
  by inspection (string literals, small integer arithmetic) are authored
  directly from the corpus program and the Q14 formatting rule at P2.
  Entries whose output is computed (checksums: `a22`–`a24`) are
  **captured** from the dev-JIT tier at P2 after the P2 review, each with
  a tracking entry recording the capture.
- Confirmation: a golden is **frozen** only when P3's AOT tier reproduces
  it byte-exactly — two independently-executed paths through the shared
  lowering must agree. Until freeze, goldens are provisional and a
  disagreement reopens the entry, never auto-updates it.
- Golden-change procedure (after freeze): a change must (1) cite the
  language rule that defines the new bytes and (2) land as an entry in
  the phase tracking file. Goldens are never edited as a side effect of
  making a test pass.
- The standing differential gate (from P3, the default `cargo test`
  path): dev-JIT output ≡ AOT output ≡ golden, byte-exact, every entry
  that has a golden.

## 3. Pre-registered criteria

- **P3 mobile link spike — kill criterion**: P3 opens with the spike. A
  minimal program (arithmetic + a runtime call + printed result) compiled
  through `cranelift-object` must produce a valid object and link cleanly
  with the platform linker for both device triples:
  `aarch64-linux-android` (NDK clang) and `aarch64-apple-ios` (Xcode
  clang). Compile+link is the whole criterion; simulators and emulators
  are not used, and run-level verification on mobile hardware is not
  required — execution semantics are carried by the host-side
  differential gate. Failure at the object or link level for either
  platform → **ship tier reverts to C emission**; dev tier stays
  Cranelift JIT. Environment note: the iOS half requires a Mac; the
  Android half runs on any host via NDK.
- **P4 performance gate**: the baseline is a hand-written C
  implementation of the a22 workload (same shape as corpus.md §4),
  compiled with the platform C compiler at `-O2`, measured on the same
  machine in the same session as the language runs and recorded in the
  tracking file when P4 opens. Criteria: ship-AOT within **1.5×** of the
  C baseline (eval median); dev-JIT within **4×** of the same baseline.
  Failing either reopens the backend decision with the measurement as
  the named criterion.

## 4. Milestones and gates

| # | Deliverable | Gate |
|---|---|---|
| P1 | Semantic checker + typed HIR | All 14 reject entries (r01–r14, plus any added since) rejected with rule-specific diagnostics at TS positions; accept corpus checks clean |
| P2 | Runtime crate + HIR→CLIF + JIT; goldens authored/captured (§2) | Run set (a01–a24) matches goldens under dev JIT |
| P3 | Mobile link spike; AOT objects + link + run; hot reload demo; standing differential gate; goldens frozen | Spike passes both targets or fallback is invoked and recorded; run set matches goldens under AOT; JIT≡AOT≡golden is the default `cargo test`; reload demonstrated on a run-set program |
| P4 | Performance gate | §3 criteria |
| P5 | C-header binding slice: mirror generator from a neutral synthetic header (all five plan-§4 patterns), `offsetof` assertion suite, headless end-to-end slice on both forms, interop corpus entries | Slice green headless; layout suite green on dev targets |

## 5. Conventions

CLAUDE.md code conventions apply to all crates (`compiler/`, `runtime/`):
no panics in library code, `///` docs + `#![warn(missing_docs)]`,
`#[must_use]`, `#[non_exhaustive]`, SAFETY comments on every unsafe impl,
unit tests with every public API, one module per area.
