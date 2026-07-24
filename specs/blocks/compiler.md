# Compiler and runtime — contract

Status: Rev 15, 2026-07-24 (Rev 0: 2026-07-22; Rev 1 moves the mobile link
spike from P3 to P0.5 — plan §8; Rev 2 adds the §6 P1 checker contract;
Rev 3 adds the §7 P2 runtime/JIT contract; Rev 4 adds the §8 P3
AOT/reload contract; Rev 5 scopes trap recovery; Rev 6 adds the §9 P4
measurement methodology; Rev 7 adds the §10 P4.1 optimization contract;
Rev 8 makes the ship tier C emission — §11; Rev 9 adds the §12 P5 binding contract; Rev 10 scopes dev-tier boundary-struct marshaling to arm64 — §12.3a; Rev 11 makes the crate build's C compilation target-portable so the workspace builds on Windows-MSVC — §11a; Rev 12 makes the runtime C toolchain clang-portable — §11b — and extends dev-JIT struct-by-value marshaling to Win64 — §12.3a — for a test-green Windows-x64 gate; Rev 13 inlines emitted-C growable-array element access — §10a; Rev 14 adds the §13 P6 production-C-header interop contract; Rev 15 adds the §14 P7 async/Future + remaining-shapes contract; Rev 16 adds the §8.1b P8 ship-tier arena allocator contract; Rev 17 adds the §15 P9 stdlib pointer). Contract for
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
    double delete / use-after-delete trap in the dev tier (the dev tier
    retains the freed bytes poisoned so the trap can fire — the ship tier
    releases instead, §8.1a). `collect()` frees unreachable allocations
    and never runs unbidden.
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

### 8.1a Ship-tier manual memory is released, not retained

The dev tier realizes `unsafeDelete`/`collect` (Q6/Q7) by
**retain-and-poison**: the freed allocation's bytes stay owned by the
Context and its header is stamped dead, so a stale handle *traps* instead
of reading reused memory (§7). That retention is the price of the dev
tier's trap-on-use-after-delete guarantee.

The **ship tier does not owe that guarantee** — in AOT, double delete and
use-after-delete are undefined (Q6; invariant 6, trusted scripts). So the
ship tier **returns the allocation to the system allocator immediately**:
`unsafeDelete` (and a `collect` sweep) free the backing storage and drop
the Context's bookkeeping entry for it, rather than retaining a poisoned
corpse.

- **Soundness / gate-safety.** For a *correct* program — one that never
  reads a handle after its `unsafeDelete`, and never deletes twice — the
  released and retained policies are observationally identical: the only
  difference is the state of memory the program has promised not to touch.
  Every `corpus/accept` entry is such a program by construction, so
  **dev-JIT bytes ≡ AOT bytes ≡ golden** (§8.3) is unaffected. The reject
  corpus's use-after-delete / double-delete entries are dev-tier
  diagnostics and are not run under AOT.
- **Mechanism.** The tier is identified by the Context constructor, with
  no generated-code and no C-entry change: the dev-JIT driver builds its
  Context in Rust (retaining), and the AOT host entry builds its Context
  through the runtime's C constructor (releasing). One lowering, one
  runtime; only the allocation-lifetime policy differs, selected at
  Context creation.
- **Why it matters (measurable).** Retention makes the Context's
  allocation table grow monotonically for the whole run: a program that
  builds and frees N reference-class instances in sequence leaves N dead
  entries behind, so each later allocation works against an ever-larger
  table (cache-hostile, superlinear). Release bounds the table at the live
  set. Exit criterion: (1) §8.3 stays byte-exact on the reference-class
  entries (including `a16` collect); (2) a runtime unit test asserts that
  in ship mode a deleted allocation leaves **no** table entry (live count
  and table size both drop), where the dev-mode test still observes the
  poisoned-corpse trap. The `tree` benchmark's per-allocation cost, flat
  in C and superlinear under retention, is the informal corroboration, not
  a gate.

### 8.1b P8 — ship-tier allocator: Context-owned arena, size-class free lists

§8.1a removed retention; the remaining ship-tier allocation cost is the
**per-allocation bookkeeping map** (measured: on the `tree` workload's
30×131071 alloc/delete pairs, the map plus its bookkeeping is ~75% of the
runtime's allocation overhead; the 32-byte-zeroed-with-header allocation
shape itself is ~+17% over the C baseline's bare `malloc`/`free`). The
ship tier therefore drops the per-allocation map from the hot path.

**Scope: ship tier only** (`Context::new_releasing`). The dev tier keeps
the map and retain-and-poison unchanged — the map is what funds its
trap-on-stale-handle diagnostics (§8.1a). One runtime, two allocation
policies, selected at Context construction as today; no generated-code,
lowering, or `sub_rt_*` ABI change.

- **Mechanism.** The ship Context owns memory in chunks. Small
  allocations (header + payload up to a largest size class) are carved
  from **per-size-class chunks** by bump pointer; `unsafeDelete` pushes
  the block onto that class's LIFO free list; the next same-class `alloc`
  pops it. Allocations above the largest class are carried as individual
  system allocations with their own Context record (they remain
  enumerable). Context drop frees chunks and large records wholesale —
  Context-scoped memory (invariant 2) is preserved.
- **Header.** The 16-byte block header (state word, class id) is kept and
  additionally carries what tracing needs (payload size or size class);
  payload alignment stays 16.
- **Zeroing.** `alloc` returns a zeroed payload in every case, including
  free-list reuse — conservative tracing and language zero-init rely on
  it.
- **Membership is exact.** The conservative scan and `collect()` need
  "is this word a managed payload address?". The test must never
  identify an address as a managed block unless it is one: chunk-range
  lookup, block-grid alignment within the per-class chunk, bump-watermark
  bound, and a live header state — all four. A false positive that lets
  the sweeper treat arbitrary memory as a block is memory corruption, not
  conservatism.
- **`collect()`** (Q7, explicitly invoked only) still works on the ship
  tier: mark from roots/shadow/interned as today; sweep by walking each
  chunk's blocks linearly (bump watermark bounds the walk) plus the large
  records; unreached live blocks go to their free list (or are freed, for
  large records). Mark state lives in the block, not in a map.
- **Q6 amendment.** §8.1a described ship-tier double delete as "presents
  as an absent entry, a silent no-op" — that was a property of the map
  implementation, not of the contract. Under the arena, double delete and
  use-after-delete remain **undefined** on the ship tier (Q6, trusted
  scripts) and may corrupt the allocator; the dev tier remains the
  diagnosing tier and still traps both.
- **Retired array blocks** (§8.1a array growth) flow through the same
  free-list/large-record path.

**Exit criteria (pre-registered):**

1. Ship `tree` ratio **≤ 2.0× C** on the arm64 reference machine
   (from 5.11×), same runner and methodology (§9); no other workload's
   ship row regresses by more than 5% beyond run noise.
2. The standing gate (§8.3) stays byte-exact on every corpus entry —
   including `a16` (collect) and the reference-class entries — on both
   tiers.
3. Runtime unit tests, same commit: (a) ship alloc→delete→alloc reuses
   free-listed storage without chunk growth over N cycles; (b) Context
   drop frees every chunk and large record (no leak, asserted by a
   counting hook or chunk count); (c) ship `collect()` frees unreachable
   blocks and keeps rooted ones (arena edition of the existing tests);
   (d) the dev-tier trap tests (double delete, stale handle) pass
   unchanged.
4. Inspection aids (`is_live`, `live_count`) remain functional on both
   tiers.

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

## 10a. Emitted-C growable-array element access is inlined

The ship-tier C emitter (§11) lowered a **growable-array** element access to
an opaque call into the runtime staticlib (`sub_rt_array_ptr`), which the
host C compiler cannot inline, cannot prove the bounds branch of, and around
which it will not vectorize. The **FixedArray** path was already inlined
(`base + idx*elem`), so only growable arrays paid this. Measured on
x86-64/Windows the opaque call alone cost ≈18% of the a22 emitted-C time
(17.2→14.0 ms at `-O2`); on arm64 it is latent because the surrounding loops
vectorize regardless — which is why the gap showed up only on x86
(`specs/tracking/windows-portability.md`).

The emitter now inlines the **in-bounds fast path** — a header-layout
pointer computation `data + (int64)idx*elem_size` guarded by an inlined
`0 <= idx < len` branch — and delegates only the **out-of-bounds case** to
`sub_rt_array_ptr`, so the trap and its exact dynamic message stay
byte-identical to the runtime path (the runtime is still the sole producer
of the trap). This mirrors the FixedArray inline form and the standard AOT
technique of keeping the bounds check but making array element access a
header-inlinable operation rather than an opaque out-of-line call. This is
a **C-emitter** optimization only; the dev-JIT tier is
unchanged (it targets iteration speed, invariant 3), and dev-JIT ≡ ship-C
equivalence is preserved by construction.

The emitted C mirrors the runtime's committed array ABI — `ArrayHeader`
(`runtime/src/context.rs`, `#[repr(C)]`):
`{ u64 len; u64 cap; u64 elem_size; u8* data; }`, invariant 1 — as a C
`SsArrayHeader` typedef. The coupling is machine-verified two ways: a runtime
test pins the `ArrayHeader` field offsets (0/8/16/24), and the standing
differential gate (dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact) fails
immediately on any layout drift, since a wrong offset reads garbage.
Closing the x86 gap the rest of the way (to the arm64 1.05×) is a value-copy
/ SIMD concern beyond this change, tracked separately.

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
stated dev-tier host (§1). This is the *compilation* contract only; the
C-invocation sites that run while tests execute are §11b, and the dev-JIT
struct-by-value ABI is §12.3a. The bench harness (`benchmarks/src/bin/perf-gate.rs`)
compiles C only when the benchmark is run (no test drives it), so it is out
of the standing test gate; it takes the same clang path (§11b) and is
verified by running it, not by the suite.

## 11b. C toolchain at runtime is clang, located portably

Three paths invoke a C toolchain while the standing gate runs — the two
ship-C AOT runners (`codegen/src/aot.rs` `run_aot`/`run_c_aot`) and the
`offsetof` layout probe (`codegen/tests/offsetof_layout.rs`). They compile
and link the emitted ship C (or a layout probe) and must reproduce the
§11-pinned ship semantics exactly, so they invoke **clang** — the compiler
§11 pins, with its flags (`-std=c11 -O2 -fwrapv -ffp-contract=off`) — not a
target-default driver whose signed-overflow or fp-contraction behaviour
would diverge from the goldens for the wrong reason. This is why clang, not
the MSVC `cl` driver, is used on Windows even though `cl` is what §11a's
crate build selects for the plain-C-ABI synthetic callee. clang's GNU-style
driver flags are identical across Unix and Windows; on Windows clang targets
`*-pc-windows-msvc` and links through the installed MSVC linker, so it
consumes the MSVC-ABI runtime staticlib and object without translation.
Resolution: `$CC` if set, else `clang` on `PATH`, else — on Windows — the
standard LLVM install (`%ProgramFiles%\LLVM\bin\clang.exe`); a missing clang
fails the run, never skips it (§8.3). Two host-shape details are
target-aware: the linked executable carries the host executable extension
(`.exe` on Windows), and the on-demand runtime staticlib is named by target
convention — `libsubscript_runtime.a` on Unix, `subscript_runtime.lib` on
`*-pc-windows-msvc` (`SUBSCRIPT_RUNTIME_STATICLIB` overrides resolution
entirely).

Two more Windows-only link/output details are required for the byte-exact
gate.
(1) A manual clang link of the runtime staticlib must add the Windows system
import libraries `rustc` supplies automatically (`kernel32`, `ntdll`,
`userenv`, `ws2_32`, `dbghelp` for the current toolchain — matched to
`rustc --print native-static-libs`); `cargo` links them for `rustc`, a hand
clang link does not. (2) A committed host entry C that writes the sink to
stdout sets that stream to binary mode (`_setmode(_fileno(stdout),
_O_BINARY)`, `_WIN32`-guarded) so the MSVCRT text mode does not translate
`\n` to `\r\n` and corrupt the byte-compared output; a no-op on every other
platform.

The benchmark harness (`benchmarks/src/bin/perf-gate.rs`, with its committed
`benchmarks/a22-baseline.c` and `benchmarks/aot-entry.c`) is a fourth clang site with
the same treatment — clang location, `.exe` suffix, Windows system libs on
the staticlib links, and binary-mode stdout in both committed C entries so
each subject matches the frozen golden. Its C entries also read the timed
span from `QueryPerformanceCounter` on Windows (the MSVC UCRT has no
`clock_gettime`/`CLOCK_MONOTONIC`), converted to nanoseconds by
overflow-safe integer arithmetic — the same monotonic span, and since every
subject is timed the same way the cross-subject ratio is timing-method
independent. It is not gate-driven, so it is verified by running the
benchmark, not by `cargo test`; the §3 performance thresholds it reports are
machine- and toolchain-dependent (the recorded ship-tier figures are the
reference setup's, §11).

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

### 12.3a Dev-tier boundary-struct marshaling: AAPCS64 and Win64

The ship tier is arm64-only C emission (§11), where the platform C
compiler performs all boundary-struct argument marshaling and is correct
by construction. The dev JIT must hand-build the C-ABI call, and passing
a boundary **struct by value** across a foreign call is ABI-specific. The
marshaler branches on the **target ABI**, not merely the architecture,
because x86-64 hosts split by OS: `x86_64-pc-windows-msvc` is Win64,
`x86_64-unknown-*` is SysV, and the two disagree on struct passing.

Implemented and verified:

- **AAPCS64 (arm64)**: a ≤16-byte struct is packed into registers (its
  components as arguments); a larger one is passed by reference to a caller
  copy (AAPCS64 B.4).
- **Win64 (`x86_64-pc-windows-msvc`)**: a struct whose total size is
  exactly 1, 2, 4, or 8 bytes is passed **by value in one integer
  register** — the whole struct as a single packed integer of that width,
  with no HFA/float-register special case and no multi-register packing;
  every other size is passed **by reference** to a caller copy. (A callback
  field expands to trampoline+binding = 16 bytes, so any struct carrying
  one is by-reference on Win64.)

On any host whose ABI is not one of these — x86-64 SysV is the open case —
lowering a foreign call that passes a boundary struct by value remains a
**loud codegen error**, never a silent mis-marshal, since dev-JIT ≡ ship-C
equivalence is otherwise unverifiable there. SysV dev marshaling is the
remaining tracked follow-up (`specs/tracking/windows-portability.md`,
`specs/tracking/p5-interop.md`).

A length-carrying **string view** (`{ const char*; size_t; }`) and a
**`(pointer, count)` array descriptor** (`{ const T*; size_t; }`) are each
a 16-byte C aggregate passed **by value**, so they take the same
target-specific path as any boundary struct — corrected from the earlier
claim that `(ptr,len)` pairs are target-neutral, which held only because
AAPCS64 and SysV both happen to pass a 16-byte two-pointer aggregate in two
registers (the same two argument slots the pair would occupy). Win64
disproves it: a 16-byte aggregate is passed **by reference**, so the dev
JIT must build the descriptor in a caller slot and pass its address, not
two registers. Only genuinely scalar/pointer boundary args — a single
handle, `object|null`, a lone pointer — are target-neutral. Each ABI is
validated by the standing differential gate (dev-JIT ≡ ship-C-AOT ≡ golden)
on a host of that ABI: the AAPCS64 path on arm64, the Win64 path on
Windows-x64.

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

## 13. P6 — production-C-header interop (host-agnostic)

Owner decision 2026-07-24. P5 proved every interop *pattern* on a small
synthetic header; P6 makes the toolchain bind an **arbitrary production C
header** (of the scale and shape of a real graphics/OS C API — ~200
functions, ~100 structs, preprocessor, attributes, doc comments). The
language stays **host-agnostic** (invariant 4): no external API is named,
depended on, or committed; the reference sweep stays zero. Capability is
proven on a neutral synthetic fixture that reproduces every production-C
shape; pointing the tool at a specific real header is a **local,
uncommitted** step the tool supports for any header path.

Committed evidence names no external project. A real production header
may be bound locally to demonstrate the capability; that demonstration
is not committed and committed write-ups describe it generically (by
scale/shape, never by API name).

### 13.1 Real C parser (replaces the fixture parser)

The `bindgen` crate's narrow fixture parser (`cparse`) is replaced by a
**libclang-based frontend** (`clang-sys`, pinned; libclang resolved at
build/run time with a documented env override). It parses real C:
preprocessor (`#define`/`#if`/`#include`), function/nullable attributes,
doc comments, `typedef`, nested structs, function-pointer typedefs,
`static const` constants, enums, and flag typedefs. Gate: it regenerates
the existing `corpus/interop/interop.generated.d.ts` **byte-identically**
(proving the new frontend is a superset of the old), and parses a new
neutral fixture carrying the production-C features above.

### 13.2 New binding shapes

- **Descriptor-embedded `(count, pointer)` arrays.** Production headers
  spell arrays as adjacent fields *inside* a larger struct
  (`size_t <n>Count; const T* <n>;`), not as a standalone two-field
  descriptor. The mirror generator recognizes the embedded pair **only in
  the exact layout the lowering can reconstruct: the `size_t` count field
  immediately precedes the `const T*` pointer field (count-first,
  contiguous)**, and maps the pointer field to `T[]` with the count
  elided; zero-copy lowering as in §12 / a26 / a31. Any other spelling
  (pointer-first, or a non-adjacent count) is **not** recognized as an
  embedded array — the bare `const T*` field is then an unmapped boundary
  type and the mirror generator **fails loud** (never a silently
  wrong-marshaled mirror). The recognizer accepts exactly what both tiers
  fill; the mirror discards C field offsets, so it must not accept a
  layout the lowering cannot honor.
- **Flag typedefs.** `typedef <intN> XFlags;` + `static const XFlags
  X_A = …;` → a `uXX` alias plus `declare const` members, combinable with
  `|` (Q18), proven end-to-end (declare, combine, pass to a foreign call,
  observe).
- **Untyped bulk-data facade.** For a `void const* data, size_t size`
  (byte-size) API, the documented path is a thin typed C facade taking a
  typed slice descriptor (§ a31), bound as `T[]`, zero-copy, both tiers.
  A fixture proves the untyped API + facade + count→bytes conversion.

### 13.3 Async callback model

Production callbacks register a callback-info now and fire later
(host-driven), unlike P5's synchronous single fire (a28). P6 proves a
**deferred** fire: the host harness stores the registration and invokes
it after the registering call returns; the userdata-lifetime rule
(userdata must outlive the registration) is enforced or the misuse traps
with a stated diagnostic. Corpus entry + both-tier golden.

### 13.4 Scaled layout proof

The neutral fixture reaches production scale/complexity (intrusive
chains, by-value string views, nested structs, flag typedefs,
descriptor-embedded arrays, dozens of structs); the `offsetof` suite
(§12.3) asserts language layout == platform C compiler for **every**
mirrored struct at that scale.

### 13.5 Generic header CLI + local capability demo

`subscript-bindgen --header <path>` runs the libclang frontend on any
header and emits the mirror. The capability is demonstrated in-session
on a real production header (mirror produced, an `offsetof` spot-check
against the platform C compiler); this run is **not committed** and the
committed record describes it by scale/shape only. The committed proof
is the neutral fixture passing every gate.

### 13.6 Staging

- **P6.1** — libclang frontend replacing `cparse`; byte-identical
  regeneration of the existing mirror; neutral fixture with production-C
  features (macros/attributes/doc-comments/static-const/flag-typedef)
  parsed. Foundation.
- **P6.2** — the §13.2 shapes (descriptor-embedded arrays, flags,
  untyped-data facade): fixture + corpus + both-tier gate.
- **P6.3** — §13.3 async model, §13.4 scaled offsetof, §13.5 generic
  CLI + local real-header capability demo.

### 13.7 Gate

The neutral production-scale fixture binds end-to-end: the mirror
regenerates byte-identically, every mirrored struct's layout == the C
compiler's, the new shapes (embedded arrays, flags, facade, async) each
have a passing both-tier corpus entry with a committed golden, and the
generic CLI binds an arbitrary header path. Reference sweep clean (no
external-API names in tracked files); invariant 4 intact.

## 14. P7 — async/Future model and remaining production shapes

Owner decision 2026-07-24. P6 binds a production C header's structure;
P7 closes the incremental interop gaps needed for the **common
main-thread-driven async / Future model** a production GPU C API uses,
and the remaining scalar/return/out shapes. Host-agnostic (invariant 4):
proven on a neutral synthetic fixture; no external API named or
committed; reference sweep stays zero.

The model P7 targets (as a real production GPU C API spells it): an async
op **returns a future** (a small by-value `{u64 id}` struct), taking a
**callback-info** value `{ mode; callback; userdata1; userdata2 }`; the
host later drives completion with a **wait/process-events** call that
takes an **out-array** of `{ future; bool completed }` the callee writes.
The callback fires on the pump/wait thread (synchronous, same thread) —
which is the a35 deferred-fire mechanism, already proven.

### 14.1 Chained integer/flag aliases

The emitter follows a `typedef → typedef → integer` chain to the
underlying sized type: `typedef uint32_t B; typedef B X;` → `type X =
u32` (and the flag-alias + `declare const` form when members exist). P6.2
handled one-level aliases and **fails loud** on two-level; P7.1 resolves
the chain (production GPU C APIs commonly spell flags as a two-level
alias). Still fail loud if the chain does not bottom out in a mapped
integer.

### 14.2 By-value boundary-struct return

A foreign function may **return a boundary value class by value** (e.g.
`SubFuture { u64 id }`). Both tiers marshal the struct return per the C
ABI (small structs in registers, larger via `sret`), subject to the
§12.3a arch-gate for the by-value aggregate ABI. Today foreign returns
are scalar/handle only (`lower/func.rs` rejects a non-scalar boundary
return); P7.2 adds the struct-return path. The returned value class is
then an ordinary in-language value (its fields readable, e.g. the future
id).

### 14.3 Out / mutable array and out fields

A foreign function may take a **mutable `T[]`** (or a boundary struct with
a callee-written field) that the callee writes back; the script reads the
written values after the call. Rule: the array/struct storage is the
caller's; the callee borrows it mutably for the call's duration and may
write; the caller observes the writes after return (no copy back — the
callee wrote the caller's own storage). Both tiers pass the same
`(ptr,count)` / pointer and the writes land in the language array/struct.
This is distinct from the const-borrow of a26/a31 (state the surface
spelling that marks an out/mutable array parameter).

### 14.4 Two userdata slots

The callback trampoline and the runtime `CallbackBinding` carry
**two** `void*` userdata slots (`userdata1`, `userdata2`), both delivered
to the language callback (each `object | null`, narrowed with `as`). P5.2b
carried one; P7.2 extends it. The Context-held-binding lifetime rule
(§13.3) is unchanged.

### 14.5 Composed Future-shape async (capstone)

A neutral fixture reproduces the whole model: an async op returns a
future (14.2), taking a callback-info `{ mode; callback; userdata1;
userdata2 }` (14.4); a host wait/process-events call takes an out-array
of `{ future; bool completed }` (14.3) and fires the registered callback
on the calling thread with both userdata. A corpus entry (a36+) drives it
end-to-end with a committed golden, byte-exact on both tiers.

### 14.6 Permanent non-goal — spontaneous (arbitrary-thread) callbacks

A production async model's *spontaneous* mode fires a callback on an
arbitrary thread at an arbitrary time. This is **permanently out of
scope**: the Context and the callback trampoline are single-threaded by
design (P5.2b soundness relies on same-thread synchronous invocation
under one Context; scripts are single-threaded, invariant 6). The
supported model is the **main-thread wait/process-events** path (14.5),
which fires synchronously on the caller thread. Binding a header that
offers a spontaneous mode is fine — a program simply must not select it;
the toolchain need not enforce this (trusted scripts).

### 14.7 Staging and gate

- **P7.1** — §14.1 chained aliases, §14.2 by-value struct return, §14.3
  out/mutable arrays: each a neutral fixture shape + both-tier corpus
  entry + golden; new structs pass the offsetof suite.
- **P7.2** — §14.4 two userdata + §14.5 the composed Future-shape async
  capstone: corpus entry, both-tier golden.
- **P7.3 (optional)** — re-run the generic `--header` CLI locally on a
  real production GPU C API header and report the coverage gain (how far
  the mirror gets now, what still fails loud), by scale/shape only, not
  committed.

Gate: each shape has a passing both-tier corpus entry with a committed
golden; the composed async entry passes both tiers; the mirror
regenerates byte-identically; still-unmapped constructs fail loud;
reference sweep clean; §14.6 documented as a permanent non-goal.

## 15. P9 — standard library (`Math`, `Date`)

Contract in `specs/blocks/stdlib.md`; collision resolutions Q19/Q20 in
`collisions.md` §2. Compiler surface: ambient-namespace intrinsic calls
(`Math.<fn>`), checker-folded constant member reads, and an ambient
nominal value type erasing to `i64` (`Date`); every runtime-backed
operation lowers to an opaque `sub_rt_math_*`/`sub_rt_date_*` call on
both tiers (never a direct libm emission — clang constant-folds libm at
`-O2`, a cross-tier divergence hazard, `stdlib.md` §0.2). Gate:
`stdlib.md` §5.
