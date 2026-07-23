# Compiler and runtime — contract

Status: Rev 11, 2026-07-23 (Rev 0: 2026-07-22; Rev 1 moves the mobile link
spike from P3 to P0.5 — plan §8; Rev 2 adds the §6 P1 checker contract;
Rev 3 adds the §7 P2 runtime/JIT contract; Rev 4 adds the §8 P3
AOT/reload contract; Rev 5 scopes trap recovery; Rev 6 adds the §9 P4
measurement methodology; Rev 7 adds the §10 P4.1 optimization contract;
Rev 8 makes the ship tier C emission — §11; Rev 9 adds the §12 P5 binding contract; Rev 10 scopes dev-tier boundary-struct marshaling to arm64 — §12.3a; Rev 11 makes the crate build's C compilation target-portable so the workspace builds on Windows-MSVC — §11a). Contract for
the plan's P0.5–P5 phases
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
  construction. *(Superseded for the ship tier by Rev 8 / §11: the ship
  tier is HIR→C→`clang` (LLVM), a second lowering, after P4 measured
  Cranelift AOT at 23× a C baseline. dev/ship agreement is then
  established by verification — the standing gate — not by construction.
  The dev tier is unchanged: Cranelift JIT with hot reload.)*
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

- **P0.5 mobile link spike — kill criterion**: the spike has no
  dependency on the language (it emits a fixed program), so it runs
  immediately after P0, before any compiler investment. A minimal
  program (arithmetic + a runtime call + printed result) compiled
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
| P0.5 | Mobile link spike (`cranelift-object` emitter + runtime stub + link script + host-side object-parse tests) | Spike passes both targets or fallback is invoked and recorded |
| P1 | Semantic checker + typed HIR | All 14 reject entries (r01–r14, plus any added since) rejected with rule-specific diagnostics at TS positions; accept corpus checks clean |
| P2 | Runtime crate + HIR→CLIF + JIT; goldens authored/captured (§2) | Run set (a01–a24) matches goldens under dev JIT |
| P3 | AOT objects + link + run; hot reload demo; standing differential gate; goldens frozen | Run set matches goldens under AOT; JIT≡AOT≡golden is the default `cargo test`; reload demonstrated on a run-set program |
| P4 | Performance gate | §3 criteria |
| P5 | C-header binding slice: mirror generator from a neutral synthetic header (all five plan-§4 patterns), `offsetof` assertion suite, headless end-to-end slice on both forms, interop corpus entries | Slice green headless; layout suite green on dev targets |

## 5. Conventions

CLAUDE.md code conventions apply to all crates (`compiler/`, `runtime/`):
no panics in library code, `///` docs + `#![warn(missing_docs)]`,
`#[must_use]`, `#[non_exhaustive]`, SAFETY comments on every unsafe impl,
unit tests with every public API, one module per area.

## 6. P1 checker contract

Observable obligations only; internal design is the implementer's.

- Crate: `compiler/` (workspace member). Public API at minimum:
  check a set of source files (a19 is two files) → either a typed HIR
  module or a non-empty diagnostic list. `Result`-based; no panics.
- **Diagnostics** carry: a stable rule code (table below), a message, and
  a TS position (file, 1-based line and column) pointing at the
  offending construct. Messages are free-form; codes and positions are
  the tested contract.
- **Rule codes** (stable identifiers; never renumber):

| Code | Rule | Source | Reject entries |
|---|---|---|---|
| S001 | `any` banned | founding | r01 |
| S002 | no dynamic code evaluation (`eval`, `new Function`) | founding | r02, r05 |
| S003 | no prototype mutation | founding | r03 |
| S004 | nominal types are closed | founding | r04 |
| S005 | no structural substitution | C1 | r06 |
| S006 | value classes do not inherit | C2 | r07 |
| S007 | bare `number` rejected | C3 | r08 |
| S008 | integer literal out of range | C4 | r09 |
| S009 | capturing lambda may not escape | C5 | r10 |
| S010 | exceptions are not in the language | C6 | r11 |
| S011 | unions limited to `T \| null` | C7 | r12 |
| S012 | `undefined` banned | C7 | r13 |
| S013 | no `async` / event loop | C8 | r14 |

  Constructs outside the decided surface (e.g. non-whitelisted
  `Array.prototype` / `string` members — collisions.md Q4/Q5) are
  rejected under a catch-all code S100 (`outside the decided surface`)
  with the offending member named in the message.

- **Typed HIR**: every expression carries its resolved type (sized
  numerics distinct from each other; value classes distinct from
  reference classes; nominal identity preserved) and a TS position.
  Monomorphization of the a12 generic shapes may happen in HIR or be
  deferred to P2 lowering — implementer's choice, recorded in the
  tracking file.
- **Gate tests** (in the default `cargo test`): one integration test
  iterates `corpus/reject/` and asserts, per entry, the expected rule
  code (table above) and that the position lands in the entry's file at
  the offending line; one iterates `corpus/accept/` and asserts zero
  diagnostics and a well-formed HIR per entry.
- SWC front end pinned in `Cargo.lock` (`swc_common 5.0.1`-compatible
  family; the Cranelift 0.125.4 serde constraint —
  `specs/tracking/p0.5-mobile-link.md` — binds the choice).

## 7. P2 runtime and JIT contract

Observable obligations only; internal design is the implementer's.

- Crates: `runtime/` (workspace member — the single runtime crate of §1;
  every function callable from generated code is `extern "C"` with a
  stable signature). CLIF lowering and the JIT driver may live in
  `compiler/` or a new member; the choice is recorded in the tracking
  file.
- **Execution API**: given a checked program (P1 HIR), run the exported
  `main(): void` under `cranelift-jit` and return the captured stdout
  **bytes** (print writes to a runtime-owned sink, not the process
  stdout, so tests compare bytes exactly). Outcome is `Result`: normal
  completion with output, or a trap report (rule + TS position); a trap
  never aborts the host process and no signal/SEH handling is used.
- **Runtime semantics** (the run set exercises all of these):
  - Context memory: reference classes, arrays, strings, coroutine
    frames are Context allocations. `unsafeDelete` frees immediately;
    double delete / use-after-delete trap in the dev tier. `collect()`
    frees unreachable allocations and never runs unbidden.
  - Strings: immutable UTF-8 `(ptr, len)`; `length` = byte length
    (`i32`); `slice(start, end)` byte offsets, traps off a UTF-8
    boundary; `+`/template concatenation; `===`/`!==` by content.
  - Arrays: `length`, indexing (OOB traps), `push`, `pop` (empty-pop
    traps).
  - Numerics: two's-complement wrap on i32/u32/i64/u64 arithmetic;
    `as` conversions truncate/wrap per C; **f32 arithmetic is performed
    at f32 precision** (never computed in f64 and rounded — the a22
    checksum depends on it); 64-bit bitwise ops are true 64-bit (Q18).
  - **Q14 formatting** (template-literal interpolation, shared by both
    tiers from P3): integers in decimal; `f32`/`f64` by shortest
    round-trip; spellings exactly `-0`, `NaN`, `Infinity`, `-Infinity`
    (a formatter that spells infinity `inf` must be mapped).
  - Coroutines: CPS transform in codegen (§1); suspended state is
    Context data; `.next()` returns `{ done, value }` with `value`
    zero-initialized when `done`.
- **Goldens** (procedure in §2): `corpus/accept/<id>.expected` for
  a01–a21 are authored by inspection from the program and the Q14 rule,
  independently of the implementation, before any output comparison;
  a22–a24 are captured from the dev JIT after the P2 review. The
  differential test (default `cargo test`) compares JIT output to every
  committed `.expected` byte-exactly; a mismatch against an authored
  golden is investigated as a bug on one side and resolved by evidence,
  never by silently editing the golden.
- **Gate** (§4): run set a01–a24 matches goldens under the dev JIT.

## 8. P3 AOT, hot reload, and the standing gate

Observable obligations only; internal design is the implementer's.

### 8.1 AOT tier

- The **same** `lower_module` instantiated with `cranelift-object`
  (§1: one lowering, both tiers). A lowering change that helps one tier
  and not the other is a contract violation, not an optimization.
- Host-target AOT (the machine running CI) is the gate path: emit an
  object, link it with the runtime staticlib and a small C or Rust
  entry that calls the program's `main` and writes the Context sink
  bytes to stdout, run it, capture stdout bytes.
- Device-triple AOT (`aarch64-apple-ios`, `aarch64-linux-android`)
  must still **compile and link** for a run-set entry — the P0.5 spike
  proved a minimal program; P3 proves the real lowering's output links.
  No device execution is required (P0.5 criterion, unchanged).
- Cross-tier determinism: the AOT binary's stdout bytes must equal the
  JIT's for every run-set entry. Where they differ, the language rule
  decides which side is wrong (§2), never the golden.

### 8.2 Hot reload (dev tier)

Per §1's rules, made testable:

- **Declaration hash** over: every value/reference class (field names,
  types, order), enum member values, `FixedArray` shapes, every
  module-level variable's name and type, every function signature
  (name, parameter types, return type). Function *bodies* are excluded
  by construction. The hash is computed from the typed HIR, is stable
  across recompiles of identical declarations, and changes for any
  declaration edit.
- **Accepted swap**: same declaration hash → the per-function
  indirection table is repointed at the newly compiled bodies. Context
  state (globals, live allocations) survives; execution continues.
- **Rejected swap**: different declaration hash → the swap is refused
  with a diagnostic naming the first differing declaration; the running
  program is untouched (a refused reload never corrupts a live
  Context).
- **Stale coroutines**: a coroutine suspended in a function whose body
  was replaced is invalidated; resuming it traps with a
  `stale coroutine after reload` report carrying the resume position.
  A trap does not end the dev session: the host clears the trap record
  at the boundary and calls script again; Context state (globals, live
  allocations) is unaffected by the clear, and a stale coroutine stays
  stale (resuming it traps again). A trapped session that cannot be
  resumed would make the contract's own stale-coroutine rule kill the
  program the reload was required to keep running.
- **Frame boundary**: swaps are applied only between host calls into
  script (no swap while script code is on the stack). The demo drives
  this explicitly.
- **Demo** (a test, not a script): exercises all three cases — an
  accepted body edit whose new behaviour is observed in output, a
  rejected layout edit, and a stale-coroutine trap.

### 8.3 Standing differential gate and golden freeze

- The default `cargo test` path gains: for every run-set entry with a
  committed golden, **dev-JIT bytes ≡ AOT bytes ≡ golden bytes**.
  Byte-exact; no normalization; a missing AOT toolchain fails the test
  rather than skipping it (the gate machine is the dev machine).
- On green, the a22–a24 goldens captured at P2 are **frozen** (§2): the
  tracking entry records the confirmation, and later changes follow the
  golden-change procedure.

### 8.4 Gate (§4)

Run set matches goldens under AOT; JIT≡AOT≡golden is the default
`cargo test`; reload demonstrated on a run-set program; device-triple
link green for a run-set entry.

## 9. P4 measurement methodology

The thresholds are pre-registered in §3 and do not move. This section
pins *how* the numbers are produced, before any number exists.

- **The baseline is verified, not asserted.** The hand-written C
  program must print the same bytes as `corpus/accept/
  a22-matrix-propagation.expected`. A baseline that does not reproduce
  the frozen golden is not the same computation and the measurement is
  void. Same N, same iteration count, same LCG seed and sequence, same
  f32 arithmetic — the C source declares the correspondence in a
  comment naming the corpus entry.
- **What is timed**: the execution of the workload only — the loop the
  entry performs, measured inside the process, excluding process
  start-up, compilation, linking, JIT warm-up, and I/O. All three
  subjects (C, ship-AOT, dev-JIT) time the same span by the same
  clock class (monotonic).
- **Procedure**: at least 3 warm-up runs discarded, then at least 11
  timed runs; the reported figure is the **median**. Report the median
  and the min/max spread for each subject; a spread wider than ±20% of
  the median invalidates the run (machine too noisy) and it is redone.
- **One session, one machine**: all three subjects are measured in the
  same session on the same machine, with the machine's state described
  in the tracking entry (host, CPU, whether on AC power). Numbers from
  different sessions are never compared.
- **Compile-time is reported, not gated**: dev-tier JIT compile time
  for the entry is recorded alongside (it is the iteration-speed
  argument), but §3's 4× criterion is about execution.
- **Both outcomes are recorded.** If a threshold fails, the tracking
  entry records the measurement, the failure, and the named criterion
  reopening the backend decision (§3) — the gate is not retried with a
  different methodology.

## 10. P4.1 lowering optimization and re-measurement

Owner decision 2026-07-23, after P4 missed both thresholds
(`specs/tracking/p4-performance.md`): the measurement's dominant cost
was located in this project's own code generation, so the lowering is
optimized and the gate re-measured before the backend decision (§3) is
judged.

- **Scope — what the optimization must address**, both in the shared
  lowering so each tier gets it (§1):
  1. **Proof-based bounds-check elimination.** A check is removed only
     where the index is *proved* in range (loop induction variables
     with constant bounds, constant indices, and arithmetic over
     them). Where a proof is unavailable the check stays. Removing a
     check that could fire is a correctness defect, not an
     optimization.
  2. **Value-class copy traffic.** Copies that C2 does not make
     observable (a returned value struct written straight into its
     destination; a read-modify-write of an element that never
     escapes) are elided. C2's observable copy semantics do not
     change: `a04` remains the witness.
- **Safety net, non-negotiable**: the standing gate (§8.3) runs
  unchanged — dev-JIT ≡ AOT ≡ golden, byte-exact, all 24 entries. An
  optimization that changes any golden byte is wrong by definition.
  In addition, traps that remain reachable must still fire: the phase
  adds tests that an out-of-range index still traps with its position
  in cases the analysis cannot prove, including a dynamically computed
  index and a loop whose bound is not statically known.
- **Re-measurement**: §9's methodology and §3's thresholds are used
  unchanged — same harness, same baseline, same machine, same spans.
  No threshold moves and no methodology is re-negotiated on the basis
  of a number.
- **Judging the backend (§3)**: after the re-measurement the tracking
  entry states, with the profile, how much of the original gap was
  this project's code generation and how much survives as backend
  behaviour. The backend decision is judged against *that* figure. A
  re-measurement that still misses by a wide margin is itself the
  evidence for changing backend; one that lands near the thresholds
  is evidence for keeping Cranelift.

## 11. P4.3 ship tier — C emission (LLVM)

Owner decision 2026-07-23 (plan §8 Rev 2): the ship tier is
HIR→C→platform C compiler (`clang -std=c11 -O2 -fwrapv
-ffp-contract=off`, i.e. LLVM). The emitted C, the synthetic interop
callee, and the device-link builds all pin **`-std=c11`** (owner
2026-07-23) — the emitted dialect is verified C11 (compound literals,
`<stdint.h>`/`<stdbool.h>` types, C-ABI layout; strict
`-std=c11 -pedantic-errors` compiles it with no GNU extensions), so the
ship tier does not depend on the platform compiler's default `-std`.
`-fwrapv` makes signed overflow defined two's-complement wrap;
`-ffp-contract=off` matches the language's non-contracting f32.
Evidence: P4/P4.1/P4.2 (`specs/tracking/p4-performance.md`) — Cranelift
ship-AOT 23× a C baseline, ≈73% attributable to its scalar output;
emitted C carrying the same semantics measured 1.05×. The dev tier is
unchanged (Cranelift JIT, hot reload). The P4.2 emitter
(`codegen/src/cemit.rs`) is the a22-only spike; this phase makes it the
ship tier.

- **Coverage**: the C emitter handles the full run set a01–a24
  (reference classes, `Nullable`, lambdas/function pointers,
  generators/CPS, methods, `while`/`switch`/ternary, computed strings —
  everything the run set uses), not just a22's subset. Constructs
  outside the run set may return a clean `Err` until a corpus entry
  needs them.
- **Semantic faithfulness**: the emitted C carries the language's
  semantics exactly as the CLIF path does — C2 value copies, checked
  growable-array indexing and push growth, f32 kept in `float`, the
  P4.1 proof-based FixedArray bounds-check elimination and copy
  elision, Q14 formatting, and the trap model (a trap reports and
  returns without aborting the host, matching the runtime). It is not a
  hand-optimized rewrite; where semantics and CLIF differ the emitter
  is wrong.
- **Standing gate (replaces §8.3's Cranelift-AOT column)**: the default
  `cargo test` path becomes **dev-JIT ≡ ship-C-AOT ≡ golden**,
  byte-exact, all 24 entries. This is where dev/ship agreement is now
  established — by verification, since the two tiers are separate
  lowerings (plan §8 Rev 2). The `cranelift-object` AOT path is retained
  only as an optional extra cross-check column; its ship role has ended.
- **Device triples**: the C is cross-compiled with `clang`
  (`--target=aarch64-apple-ios` / `aarch64-linux-android` via the NDK)
  and linked, replacing the `cranelift-object` device link. Compile+link
  only, as §3 — no device execution. The P0.5 kill criterion is
  unaffected: it already passed, and C emission was its pre-registered
  fallback architecture.
- **Reuse or replicate the runtime**: the emitted C may link the
  existing runtime staticlib or emit self-contained equivalents; either
  way behaviour must match the runtime (the standing gate enforces it).
- **Gate**: run set a01–a24 matches goldens under the C ship tier;
  dev-JIT ≡ ship-C-AOT ≡ golden is the default `cargo test`; device
  triples compile and link.

## 11a. C toolchain selection is target-portable (crate build)

`codegen/build.rs` compiles the synthetic interop callee
(`corpus/interop/interop.c`) and archives it into every binary linking
`subscript-codegen`; being the crate's build step, it decides whether
`cargo build`/`cargo check` succeed at all. It selects the
target-appropriate C toolchain instead of hardcoding the Unix `cc`/`ar`
drivers: selection is by Rust target triple through the `cc` crate (already
resolved in `Cargo.lock` and present in the local registry cache —
offline-clean, no fetch), which drives the GCC/Clang driver
(`-std=c11 -O2 -fPIC`) plus `ar` on Unix targets and the MSVC toolchain
(`cl`/`lib`) on `*-pc-windows-msvc`. The `-std=c11` dialect pin (§11) is
carried across; the exact per-toolchain flag set is the implementation's
and is validated by execution, not asserted here. `CC`/`AR` overrides
remain honored where the driver accepts them.

Consequence: the workspace compiles on `x86_64-pc-windows-msvc` — already a
stated dev-tier host (§1) — not only on Unix gate machines. This is the
*compilation* contract only. The other C-invocation sites — ship-C
compile+link (`codegen/src/aot.rs`), the `offsetof` layout probe
(`codegen/tests/offsetof_layout.rs`), and the bench harness
(`bench/src/main.rs`) — run outside a build script and remain
Unix-toolchain-only; porting them, together with the §12.3a Win64 dev-JIT
struct-by-value marshaling, is the tracked follow-up for a fully test-green
x86-64/Windows gate (`specs/tracking/windows-portability.md`).

## 12. P5 C-header binding vertical slice

The language's founding purpose (plan §4): express C-ABI interop with
zero marshaling. P5 proves it against a **neutral synthetic C header**
that exercises all five interop patterns — no real-world library is
named or depended on (invariant 4; CLAUDE.md repo hygiene).

### 12.1 The synthetic header

A committed C header (`corpus/interop/<name>.h` or similar) authored for
this slice, containing only the constructs the five patterns need and
**no unions, no bitfields** (the layout-identity guarantee is about C
structs/enums/pointers/function-pointers/opaque-handles only). It
exercises, one construct per plan-§4 pattern:

1. an intrusive extension chain (a common embedded header with a `next`
   pointer and a type tag, plus ≥2 chainable extension structs);
2. a `(pointer, count)` array-pair API;
3. a length-carrying string-view struct (`{ const char*; size_t; }`,
   not NUL-terminated);
4. a callback API (function pointer + `void* userdata`);
5. an opaque handle with create/retain/release.

### 12.2 Mirror generator

A generator (`bindgen`-style, this project's own) reads the header and
emits the ambient `.d.ts` mirror per the Q13 boundary typing rules
(`specs/blocks/collisions.md` §2 Q13), already decided and binding:
opaque handles → branded empty interfaces; struct pointers and zeroable
by-value sub-layouts → `X | null`; string views → `string`; flag sets →
`u64` aliases; callback userdata → `object | null` narrowed with `as`.
The generated mirror is **never hand-edited** (CLAUDE.md core principle
6); regenerating from the pinned header reproduces it byte-for-byte
(a test).

### 12.3 `offsetof` assertion suite — the layout proof

Invariant 1 (C-ABI-identical layout) is machine-verified here, not
asserted. For every struct the mirror exposes, a generated test asserts
that the language's lowered layout matches the C compiler's:
`offsetof`/`sizeof`/`_Alignof` of each field and the whole struct, taken
from the real C header via the platform C compiler, equal the language
compiler's computed offsets/size/alignment. A mismatch fails the suite.
This runs for the dev targets (host) and is the concrete discharge of
"machine-verifiable via `offsetof` assertions" (plan §3 invariant 1).

### 12.3a Dev-tier boundary-struct marshaling is arm64-only (for now)

The ship tier is arm64-only C emission (§11), where the platform C
compiler performs all boundary-struct argument marshaling and is correct
by construction. The dev JIT must hand-build the C-ABI call, and passing
a boundary **struct by value** across a foreign call is ABI-specific
(AAPCS64 passes a >16-byte struct by reference and packs ≤16-byte
structs into registers; x86-64 SysV and Win64 differ, and float-only
structs follow HFA/HVA rules). The current JIT marshaler implements
**AAPCS64 (arm64) only**. On a non-arm64 dev host, lowering a foreign
call that passes a boundary struct by value must be a **loud codegen
error**, never a silent mis-marshal — dev-JIT ≡ ship-C equivalence is
otherwise unverifiable there. Target-aware dev marshaling for x86-64
SysV / Win64 is a tracked follow-up (`specs/tracking/p5-interop.md`);
until it lands, the dev tier's foreign-struct-by-value support is
arm64-only and says so at the point of failure. (Scalar/pointer
boundary args — handles, `object|null`, `(ptr,len)` pairs — are
target-neutral and unaffected.)

### 12.4 Headless end-to-end slice on both tiers

Corpus accept entries (a25+, numbered here) written in the language
against the generated mirror, one per pattern plus one that composes all
five, exercised headless (no GPU, no window, no external device —
CLAUDE.md core principle 4). A minimal C implementation of the synthetic
header (committed, compiled and linked into the test) provides the
callee side. Each entry runs under **both tiers** and its output is a
committed golden; the standing differential gate (§11) extends to them:
dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact. Q16 (how a corpus program
obtains a host-created handle) is decided here: the host harness creates
the handle and calls an exported entry, or the entry creates it through
the synthetic `create` — state which per entry.

### 12.5 Gate (§4)

The five patterns each have a passing headless corpus entry on both
tiers with a committed golden; the mirror regenerates byte-identically
from the pinned header; the `offsetof` layout suite is green on the dev
target. Zero real-world-library references (reference sweep clean).
