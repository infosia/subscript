# Compiler and runtime — contract

Status: Rev 25, 2026-07-27 (Rev 0: 2026-07-22; Rev 1 moves the mobile link
spike from P3 to P0.5 — plan §8; Rev 2 adds the §6 P1 checker contract;
Rev 3 adds the §7 P2 runtime/JIT contract; Rev 4 adds the §8 P3
AOT/reload contract; Rev 5 scopes trap recovery; Rev 6 adds the §9 P4
measurement methodology; Rev 7 adds the §10 P4.1 optimization contract;
Rev 8 makes the ship tier C emission — §11; Rev 9 adds the §12 P5 binding contract; Rev 10 scopes dev-tier boundary-struct marshaling to arm64 — §12.3a; Rev 11 makes the crate build's C compilation target-portable so the workspace builds on Windows-MSVC — §11a; Rev 12 makes the runtime C toolchain clang-portable — §11b — and extends dev-JIT struct-by-value marshaling to Win64 — §12.3a — for a test-green Windows-x64 gate; Rev 13 inlines emitted-C growable-array element access — §10a; Rev 14 adds the §13 P6 production-C-header interop contract; Rev 15 adds the §14 P7 async/Future + remaining-shapes contract; Rev 16 adds the §8.1b P8 ship-tier arena allocator contract; Rev 17 adds the §15 P9 stdlib pointer; Rev 18 adds the §16 P14 narrow-numerics contract — `i8`/`u8`/`i16`/`u16`/`f16`, `f16` storage-only; Rev 19 adds the §17 P16 generated-API-reference contract; Rev 23, 2026-07-26, adds the §21 P21 allocation-path contract — fault injection and per-allocation attribution, superseding §18.2e; Rev 22, 2026-07-26, adds the §20 P20 trap-site-IR contract; Rev 21, 2026-07-26, adds the §19 P19 trap-unwind-parity contract — CRITICAL; Rev 20, 2026-07-26, contracts the host `subscript_rt_ctx_*` API retroactively and adds the §18.2 trap observer §18.1a host enter/exit, §18.1b the generated host header, §18.2b `subscript_rt_ctx_clear_trap`, and §18.2d memory accounting; Rev 24, 2026-07-27, adds the §22 P24 contract for two monotonic costs under invariant 2 — the 4.25 MiB code-point table and the dev tier's cumulative-allocation sweep; Rev 25, 2026-07-27, adds §22.5 What landed, including the measured correction that the ship-tier `tree` movement is `Context`'s 104-byte growth and not this phase). Contract for
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

- **Dev-tier hosts**: Windows, macOS, and `x86_64-unknown-linux-gnu`. The
  Linux host runs the full differential gate green (§12.3a SysV dev-JIT
  struct-by-value marshaling landed 2026-08-09;
  `specs/tracking/linux-portability.md`). AAPCS64 (arm64) and Win64
  non-regression after that shared refactor: both re-verified on their
  own hosts and discharged 2026-08-09 (tracking, "Remaining gate"). The
  ship tier ships the arm64 mobile device triples (iOS, Android) and the
  desktop host targets `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
  and `x86_64-pc-windows-msvc` (§11; desktops added 2026-08-09).
- One HIR→CLIF lowering serves both tiers; dev/ship semantics coincide by
  construction. *(Superseded for the ship tier by Rev 8 / §11: the ship
  tier is HIR→C→`clang` (LLVM), a second lowering, after P4 measured
  Cranelift AOT at 23× a C baseline. dev/ship agreement is then
  established by verification — the standing gate — not by construction.
  The dev tier is unchanged: Cranelift JIT with hot reload. The diagram's
  ship-target list is superseded too — §11 ships the arm64 mobile device
  triples and `x86_64-unknown-linux-gnu`; the "arm64-only" note predates
  the latter.)*
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

**C-visible symbol prefix.** Every symbol this project defines in the C
namespace carries the project's name: `subscript_rt_*` for the runtime
API the host calls (constants `SUBSCRIPT_RT_*`, and the opaque Context
type, `subscript_rt_context`), and `subscript_*` for
everything the generated program defines — `subscript_init`,
`subscript_export_<name>`, the `subscript_main_entry` typedef, and every
emitted helper and type. *(Renamed 2026-07-29, owner decision, from
`sub_rt_*`/`ss_*`: `sub` alone was ambiguous. `ts_`/`tsc_` was
considered and rejected — it reads as an embedded TypeScript runtime,
which this project is not, and `tsc` is the name of the TypeScript
compiler the gate runs.)*

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
    frames are Context allocations. `Context.free` frees immediately;
    double delete / use-after-delete trap in the dev tier when
    freed-handle diagnostics are on, and are undefined otherwise
    (**superseded by §8.1a-1**; §8.1a made the retention unconditional,
    and it is now a setting that is off by default). `Context.collect()` frees unreachable allocations
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
  No device execution is required (P0.5 criterion, unchanged). The
  desktop host ship targets (§11: `x86_64-unknown-linux-gnu`,
  `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`) are emitted the same
  way for Cranelift-object shape parity and, being native, are
  additionally executed by the standing gate on their own hosts.
- Cross-tier determinism: the AOT binary's stdout bytes must equal the
  JIT's for every run-set entry. Where they differ, the language rule
  decides which side is wrong (§2), never the golden.

### 8.1a Ship-tier manual memory is released, not retained

The dev tier realizes `Context.free`/`Context.collect` (Q6/Q7) by
**retain-and-poison**: the freed allocation's bytes stay owned by the
Context and its header is stamped dead, so a stale handle *traps* instead
of reading reused memory (§7). That retention is the price of the dev
tier's trap-on-use-after-delete guarantee.

The **ship tier does not owe that guarantee** — in AOT, double delete and
use-after-delete are undefined (Q6; invariant 6, trusted scripts). So the
ship tier **returns the allocation to the system allocator immediately**:
`Context.free` (and a `Context.collect` sweep) free the backing storage and drop
the Context's bookkeeping entry for it, rather than retaining a poisoned
corpse.

- **Soundness / gate-safety.** For a *correct* program — one that never
  reads a handle after its `Context.free`, and never deletes twice — the
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

### 8.1a-1 Retention is a mode, not the dev tier's policy — Rev 2026-07-29

**What changed.** §8.1a above made retain-and-poison the dev tier's
standing policy. It is now **off by default**; the dev tier releases like
the ship tier, and retention becomes a mode a host switches on when it is
hunting a dangling handle.

**Evidence.** `specs/tracking/dev-retention.md` measured what the policy
costs: retained bytes per allocation are exactly **payload + 16**, across
four object shapes and both `Context.free` and `Context.collect`, growing
strictly linearly in cumulative allocations with the live set held at
zero. A particle-shaped object at 1 000 allocations per frame and 60 fps
exhausts 8 GB in **0.77 hours**.

**Decision (owner, 2026-07-29).** Memory that grows linearly and without
bound is not acceptable regardless of duration. Bounding the retention was
considered and rejected in favour of removing it from the default path
entirely: a bound narrows *how far back* a use-after-free is detected
while still costing memory, and it leaves a stale read landing on a
recycled address — undetected and silently wrong — rather than absent.
A mode is either complete or off, and says which. (**Narrowed by
§8.1a-2**: the mode now carries a size threshold and states its coverage
by class.)

**The guarantee is now conditional, and that is a real loss.** With the
mode off, double free and use-after-free in the dev tier are undefined,
as they already are in AOT (Q6; invariant 6). **A third diagnostic is
gated with them:** freeing a pointer the Context never owned traps as an
invalid free with the mode on and is a silent no-op with it off, matching
the ship tier. Naming only the first two would understate what the
default gives up. The dev tier no longer
diagnoses them by default. This is stated here rather than footnoted
because §8.1a called the trap a dev-tier guarantee and it is one no
longer.

**The mode is per Context and host-set, not compile-time.** A build flag
would force two builds of the workspace to run one `cargo test`: the trap
corpus needs the mode on in the same run where the accept corpus, the
benchmarks and the examples need it off. So it is a Context-level setting
established before the first allocation, exposed on the host C API beside
the other `subscript_rt_ctx_*` settings, defaulting to off. When on, behaviour
is exactly today's retain-and-poison, unbounded — a diagnostic session
accepts that cost deliberately.

**Consequences.**

- **No golden moves.** §8.1a's own soundness argument applies unchanged:
  a correct program cannot observe released-versus-retained, so
  dev-JIT ≡ ship-C-AOT ≡ golden is unaffected.
- **`corpus/trap/t22` and `t23`** carry `tier-policy: dev-JIT traps;
  ship-C-AOT behavior is deliberately unspecified`. They now additionally
  require the mode, and the gate enables it for them. Their trap tuples
  and `.expected` bytes must not move.
- **Dev-tier accounting** loses its retained term: `reserved_bytes`
  becomes live plus per-allocation overhead, with the mode off.
- §8.1a's "Mechanism" paragraph — the tier picks the policy at Context
  construction — is superseded: the policy is now a setting, and the tier
  only chooses its default.

**Exit criteria (pre-registered).**

1. `benchmarks/src/bin/dev-retention-probe` reports **no growth per
   allocation** with the mode off, on every shape it already sweeps.
2. The same probe with the mode **on** reproduces `payload + 16`, so the
   diagnostic path is intact rather than removed.
3. `t22` and `t23` trap with the same kind, message and position.
4. No accept or trap golden moves; the standing differential gate green;
   `tsc` clean.
5. A runtime unit test asserts both directions of the setting, as §8.1a's
   criterion (2) does for the tier.

### 8.1a-2 The mode takes a size threshold — Rev 2026-07-29

**What changed.** §8.1a-1's mode retained every freed allocation. The
setting now carries a minimum payload size: with diagnostics on, a freed
allocation is retained and poisoned only when its requested payload is at
least the threshold; a smaller allocation is released exactly as with the
mode off. Threshold 0 reproduces §8.1a-1's behaviour unchanged.

**Decision (owner, 2026-07-29).** Retention costs `payload + 16` per
freed allocation, so the mode's growth is driven by allocation count, and
small short-lived objects are the high-count class in the loops this
language targets. Recording them exhausts a diagnostic session's memory
before the session finds its fault, while the handles a session hunts are
typically to larger, longer-lived objects. The mode therefore records
larger objects only, with the boundary host-chosen.

**C API.** The setting and its threshold are established together:

```c
int32_t subscript_rt_ctx_set_freed_handle_diagnostics(
    subscript_rt_context *ctx, uint32_t enabled, uint64_t min_payload_bytes);
```

Per Context, host-set, refused (returns 0) after the first allocation, as
before. `min_payload_bytes` is ignored when `enabled` is 0. The threshold
compares the **requested payload** — the same quantity the dev
accounting's `live_bytes` sums — not the layout.

**What §8.1a-1 argued and this narrows.** §8.1a-1 rejected bounding
retention by *age* with "a mode is either complete or off, and says
which". A size threshold is a bound by *class*, and the mode now says
which classes it covers: **complete at and above the threshold,
best-effort below it.** Below the threshold, with the mode on:

- A stale handle **traps while its address remains unallocated** — the
  live-map membership check that funds the trap is already paid for and
  stays — and is undefined once a later allocation reuses the address,
  exactly as with the mode off.
- A double free likewise traps while the address is unallocated, reported
  as an **invalid free** — without a retained record the runtime cannot
  distinguish the two — and is undefined once the address is reused.
- Freeing a pointer the Context never owned traps regardless of the
  threshold: no size exists for a pointer that was never owned, and the
  bookkeeping map detects it whole.

A session that saw no trap has shown absence of stale-handle faults only
at and above its threshold. The generated host header states this beside
the setting.

**Exit criteria (pre-registered).**

1. `benchmarks/src/bin/dev-retention-probe` gains a threshold column:
   with the mode on and a threshold strictly between two swept shapes'
   payloads, shapes below it report **0.000** bytes per allocation and
   shapes at or above it report `payload + 16`; the off and threshold-0
   columns reproduce §8.1a-1's table.
2. `t22` and `t23` trap with the same kind, message and position; the
   gate enables the mode for them with threshold 0.
3. Runtime unit tests assert: release below the threshold and retention
   at and above it, visible in the accounting; the pre-first-allocation
   refusal with the new signature; a below-threshold stale handle traps
   when its address has not been reused; invalid free traps under a
   nonzero threshold.
4. No accept or trap golden moves; the standing differential gate green;
   `tsc` clean.
5. The generated host header documents the threshold and its coverage
   statement — generator-driven, never hand-edited.

### 8.1a-3 The mode takes a retention budget — Rev 2026-07-29

**What changed.** §8.1a-2 bounded retention by class; the mode now also
bounds it in total. The setting carries a byte budget: the layouts of
retained-and-poisoned allocations never sum above it. When retiring one
more allocation would exceed the budget, the oldest retained allocations
are evicted — released and forgotten — until the new one fits; an
allocation whose own layout exceeds the whole budget is released
immediately. The budget bounds diagnostic retention only; live
allocations are the program's and are never evicted.

**Decision (owner, 2026-07-29).** Above the threshold, retention is still
unbounded (§8.1a-2's header states so), and a diagnostic session that
exhausts the machine diagnoses nothing. The owner requires a hard ceiling
on the memory the mode may hold.

**What eviction means.** An evicted allocation joins the best-effort
class §8.1a-2 defines for below-threshold frees: its stale handles trap
while the address remains unallocated and are undefined once a later
allocation reuses it. Eviction introduces no new semantic class. The
guarantee becomes: **diagnostics are guaranteed for the most recently
retained frees whose layouts fit the budget, within the size class the
threshold covers; best-effort everywhere else.** Eviction is
oldest-first because a hunted stale handle is typically to a recently
freed allocation; evicting newest-first would spend the budget on the
frees least likely to matter.

**C API.** The setting, threshold and budget are established together:

```c
int32_t subscript_rt_ctx_set_freed_handle_diagnostics(
    subscript_rt_context *ctx, uint32_t enabled, uint64_t min_payload_bytes,
    uint64_t max_retained_bytes);
```

`max_retained_bytes` is literal, as the regex budget is: 0 retains
nothing (the mode becomes wholly best-effort), and a host that wants no
practical ceiling passes `UINT64_MAX`. The budget counts the retained
allocations' **layouts** — the `payload + 16` the probe reports — because
that is the memory actually held. Both parameters are ignored when
`enabled` is 0; same pre-first-allocation refusal.

**Default budget (owner, 2026-07-29): 1 GiB** (`1_073_741_824` bytes).
The parameter has no optional form in C, so the default lives in two
places: the generated header exposes it as
`SUBSCRIPT_RT_FREED_HANDLE_DIAGNOSTICS_DEFAULT_MAX_RETAINED_BYTES` for hosts
to pass, and the dev tier's own mode-enabling path (the JIT runner's
boolean parameter) uses it rather than `UINT64_MAX`. A host that wants a
different ceiling passes its own number; nothing in the runtime treats
the constant specially.

**Exit criteria (pre-registered).**

1. Runtime unit tests drive frees past the budget and assert: retained
   layout bytes never exceed the budget at any point; eviction is
   oldest-first; an evicted handle still traps while its address is
   unallocated; the newest retained handle traps as guaranteed; an
   allocation whose layout alone exceeds the budget is released
   immediately; a zero budget retains nothing.
2. The probe gains a budget setting: with the mode on, threshold 0 and a
   budget smaller than a run's cumulative frees, reserved bytes plateau
   at or below the budget while frees continue — growth per allocation
   reaches 0 after the plateau.
3. `t22` and `t23` trap with the same kind, message and position; the
   gate enables the mode for them with threshold 0 and the default
   budget — their retention is orders of magnitude below it. No accept
   or trap golden moves; the standing differential gate green; `tsc`
   clean.
4. The generated host header documents the budget, the eviction order,
   the resulting guarantee, and the default constant — generator-driven,
   never hand-edited.

### 8.1b P8 — ship-tier allocator: Context-owned arena, size-class free lists

§8.1a removed retention; the remaining ship-tier allocation cost is the
**per-allocation bookkeeping map** (measured: on the `tree` workload's
30×131071 alloc/delete pairs, the map plus its bookkeeping is ~75% of the
runtime's allocation overhead; the 32-byte-zeroed-with-header allocation
shape itself is ~+17% over the C baseline's bare `malloc`/`free`). The
ship tier therefore drops the per-allocation map from the hot path.

**Scope: ship tier only** (`Context::new_releasing`). The dev tier keeps
the map, and retain-and-poison when freed-handle diagnostics are on — the
map is what funds its trap-on-stale-handle diagnostics (§8.1a, **narrowed
by §8.1a-1**: this paragraph described the retention as unconditional). One runtime, two allocation
policies, selected at Context construction as today; no generated-code,
lowering, or `subscript_rt_*` ABI change.

- **Mechanism.** The ship Context owns memory in chunks. Small
  allocations (header + payload up to a largest size class) are carved
  from **per-size-class chunks** by bump pointer; `Context.free` pushes
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
- **Membership is exact.** The conservative scan and `Context.collect()` need
  "is this word a managed payload address?". The test must never
  identify an address as a managed block unless it is one: chunk-range
  lookup, block-grid alignment within the per-class chunk, bump-watermark
  bound, and a live header state — all four. A false positive that lets
  the sweeper treat arbitrary memory as a block is memory corruption, not
  conservatism.
- **`Context.collect()`** (Q7, explicitly invoked only) still works on the ship
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
   counting hook or chunk count); (c) ship `Context.collect()` frees unreachable
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
- **Procedure**: warm up for **at least 200 ms of measured execution
  and at least 3 iterations**, discard it, then at least 11 timed runs;
  the reported figure is the **median**. Report the median, the min/max
  spread and **every sample** for each subject; a spread wider than
  ±20% of the median invalidates that subject's timing, which is
  withheld. **A subject must report its measured warm-up time and is
  rejected below the floor.**

  *(Revised 2026-07-27; this said "at least 3 warm-up runs discarded"
  and min/max-only reporting. `benchmarks.md` Rev 3 has the evidence:
  `clang -O2` was deleting the warm-up loop outright in three of ten C
  workloads, so a count-based rule was satisfied while zero warm-up
  ran, and min/max alone could not distinguish a cold first sample from
  scattered noise. A count also cannot express "reach steady state"
  across per-iteration costs spanning 3.7 ms to 125 ms, and this
  machine's DVFS ramp is ~70 ms.)*
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
an opaque call into the runtime staticlib (`subscript_rt_array_ptr`), which the
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
`subscript_rt_array_ptr`, so the trap and its exact dynamic message stay
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
- **Ship targets**: the emitted C is compiled and linked per target,
  replacing the `cranelift-object` device link.
  - Two cross-compiled **mobile device triples** — `aarch64-apple-ios`
    (Xcode clang) and `aarch64-linux-android` (NDK clang) — compile+link
    only, as §3, no device execution.
  - **Desktop host targets** — each natively compiled, linked, **and
    executed** byte-exact by the standing gate when the gate runs on a
    host of that triple (`run_c_aot`; dev-JIT ≡ ship-C-AOT ≡ golden), so
    they are the most-verified ship targets — the mobile triples never
    execute:
    - `x86_64-unknown-linux-gnu` (added 2026-08-09). Measured:
      `subscript emit` → clang (host runtime staticlib + the §11b Linux
      system libraries) produces an x86-64 ELF PIE that runs and prints
      the golden output (`specs/tracking/linux-portability.md`).
    - `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` (owner
      decision 2026-08-09). Standing evidence at declaration: the full
      gate executes the ship-C path on the arm64 macOS reference machine
      (every suite green, every golden byte-exact) and on windows-msvc
      (53 harnesses, 904 passed, 0 failed — commit `b3b670f`), through
      the §11b/§11c host toolchains. The declaration slice adds tooling
      parity only: both triples join the retained Cranelift-object
      cross-check (`SHIP_TARGET_TRIPLES`) with per-triple object-format
      assertions (Mach-O/ARM64, COFF/X86_64), and the macOS host gets a
      native `device-link.sh` section in the Linux section's shape. The
      Windows link smoke is the standing gate itself (`run_c_aot` under
      MSVC `cl`); the shell script does not run there and no separate
      script is added.

  The P0.5 kill criterion is unaffected: it already passed, and C emission
  was its pre-registered fallback architecture.
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

**Unix clang selection (2026-08-09).** The synthetic interop fixture
(`corpus/interop/interop.h`) is clang-only by construction: it spells
`_Nullable` and `_Float16` to exercise the libclang binder. GCC rejects
`_Nullable`; clang before 15 rejects `_Float16` on x86-64 (measured on
Ubuntu 22.04: gcc 11, gcc 12, clang 14 each fail; clang 15 compiles the
fixture). The Unix default driver is therefore wrong for this fixture on a
GCC host. The fixture build scripts (`codegen/build.rs`,
`codegen/tests/native-fixture/build.rs`) must select a clang that compiles
x86 `_Float16`: resolve `$CC` first, then `clang`, then a `clang-NN` on
`PATH` newest-first; the first driver that compiles `_Float16` wins. A host
with no capable clang fails loud (§8.3), never a silent GCC fallback. This
matches §11b's runtime clang resolution and the fixture's "gate compiler is
clang" design. Evidence: `specs/tracking/linux-portability.md`.

Consequence: the workspace compiles on `x86_64-pc-windows-msvc` — already a
stated dev-tier host (§1). This is the *compilation* contract only; the
C-invocation sites that run while tests execute are §11b, and the dev-JIT
struct-by-value ABI is §12.3a. The bench harness (`benchmarks/src/bin/perf-gate.rs`)
compiles C only when the benchmark is run (no test drives it), so it is out
of the standing test gate; it takes the same clang path (§11b) and is
verified by running it, not by the suite.

## 11b. C toolchain at runtime is clang, located portably

**Windows note (2026-07-28):** §11c supersedes this section's Windows
toolchain choice — the default Windows ship-C compiler is MSVC `cl`, not
clang. The rest of §11b (system import libraries, binary-mode stdout,
staticlib name, `.exe` suffix) still applies to the `cl` path.

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

**Linux runtime system libraries (2026-08-09).** A manual clang link of the
runtime staticlib on Linux must add the platform native system libraries
`rustc` supplies automatically, the same way the Windows path adds its
import libraries. macOS hides them in `libSystem`, so the gap is latent
there and appears first on Linux: without them the link fails with
`undefined reference to exp/log/pow/sin/…` from the runtime's `f64` math
(measured). The list is the **set** `rustc --print native-static-libs` reports for the
target (`gcc_s`, `util`, `rt`, `pthread`, `m`, `dl`, `c` on
`x86_64-unknown-linux-gnu`) — the whole set, never just `-lm`; link order is
immaterial for these libraries. `runtime_system_libraries` returns this set
on Linux, empty on macOS. Evidence: `specs/tracking/linux-portability.md`.

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

## 11c. C toolchain on Windows is MSVC `cl` (supersedes §11b's Windows clang)

Owner decision 2026-07-28: on `*-pc-windows-msvc` the ship-C toolchain is
the native MSVC compiler `cl`, not clang. subscript must build on Windows
with the platform toolchain alone — no LLVM install as a prerequisite.
This supersedes §11b's choice of clang on Windows; §11b still governs the
Unix host, and clang/clang-cl remains an optional cross-check. Every
other Windows detail §11b lists — the system import libraries, binary-mode
stdout, the `subscript_runtime.lib` staticlib name, the `.exe` suffix — is
unchanged and applies to the `cl` path.

Flags: `/nologo /std:c11 /O2 /utf-8 /fp:strict`. `/utf-8` makes `cl` read
the UTF-8 sources without the CP932/ACP-dependent C4819 warning. `/fp:strict`
is the `-ffp-contract=off` equivalent — it forbids contraction and
reassociation — but the stricter mode is *required*, not merely chosen: the
emitter writes `double inf = 1.0 / 0.0;` for `Infinity`, which `cl`
constant-folds and rejects (`C2124`) under the default `/fp:precise`;
`/fp:strict` defers it to a runtime infinity (clang only warns). Output and
link syntax is MSVC's: `/Fo:<dir>\` for objects, `/Fe:<exe> -link` for the
executable, `.obj` object files, and the §11b system import libraries as
bare `.lib` names (not `-l`). The compiler and its `INCLUDE`/`LIB`/`PATH`
environment are discovered with `cc::windows_registry::find_tool`, so no
prior `vcvars` shell is needed — `codegen/src/bin/msvc-cl` is a thin shim
that applies that lookup for the `sh`-driven capstone build, which cannot
run `vcvars` itself.

**Signed-overflow soundness.** §11b pinned clang because `-fwrapv` makes
signed overflow defined two's-complement wrap, the language's semantics;
`cl` has no `-fwrapv` equivalent. MSVC does not optimize on the
signed-overflow-is-UB assumption and wraps two's-complement *(docs)*. The
guarantee is re-established the project's standing way — by verification,
not by a compile flag: the Windows standing gate runs `cl` and stays
byte-exact (dev-JIT ≡ ship-C-AOT ≡ golden, §11), so any `cl` divergence on
overflow breaks the gate. Evidence (measured 2026-07-28, MSVC 19.44):
emitted ship C for the language examples e01–e08 compiles under `cl` and
is byte-identical to the goldens, the wrapping and `as`-conversion cases
(e01) included; `engine.c` compiles under `cl` (C4819 only, silenced by
`/utf-8`).

Four constraints the `cl` path adds, all measured:

1. **The emitter must not output an empty struct.** MSVC C mode rejects a
   zero-member struct (`error C2016`); clang accepts it. The opaque-handle
   pointee is emitted today as `struct Sub_N_<Handle> {}`. It must carry
   at least one member (a single `char`), or be an incomplete type used
   only behind a pointer — the pointee is never instantiated by value in
   emitted C, so either is ABI-safe. Measured: adding a `char` member
   keeps e09/e10 byte-identical across both tiers under `cl`.

2. **A host header that spells a boundary type `_Float16`/`__fp16` fails
   loud on the `cl` path.** Emitted C never spells `_Float16` (`f16` is
   `uint16_t` storage, §16.2), so `f16` *programs* build under `cl`. But a
   bound host facade whose C source spells the type directly — the
   `corpus/interop` fixture does — cannot be compiled by `cl`: MSVC 19.44
   has no half-width float in any `/std` or language mode (measured). This
   is the §16.2 fail-loud stance, never an integer substitution. On the
   MSVC-Windows configuration the interop fixture and the two-header gate
   that binds it are therefore excluded from the gate; the clang build
   still covers them.

3. **Constraint 2's exclusion is structural, not per-test.** A test that
   names a corpus entry must obtain its native libraries from one shared
   helper whose return type expresses "this entry does not run in this
   configuration"; a call site that ignores that case does not compile.
   Rationale is measured, not stylistic: constraint 2 was first
   implemented as a `#[cfg(all(windows, target_env = "msvc"))] if
   references_interop { continue; }` guard repeated at each call site, so
   an added test that omitted the guard ran an interop entry against no
   fixture and failed. Every per-feature golden test added after the
   exclusion landed (`8c43270` onward) omitted it; measured on
   `x86_64-pc-windows-msvc` 2026-08-02, `cargo test -p subscript-codegen
   --test golden` was 7 passed / 11 failed, every failure the same
   `unresolved foreign symbol ...: no supplied native library registers
   it`. A guard that must be copied is a guard that is forgotten; the
   type system carries it instead.

   The exclusion never weakens the gate off windows-msvc: on every other
   configuration the helper supplies the fixture for every entry that
   references it, and the standing gate (§2, §11) compares the full run
   set. On windows-msvc an excluded entry is compiled and run by neither
   tier, and the run set the test reports counts only what it compared.

4. **A toolchain failure report must carry both streams.** `cl` and
   `link.exe` write their diagnostics to stdout. Unix compilers write
   them to stderr. A report of one stream only is empty on one host
   family, so the caller sees the failure and no cause. Measured
   2026-08-05 on windows-msvc: `run_c_aot_with_native_libraries` failed
   for every program of a host facade that binds a C header, and printed
   `compiling/linking the emitted C failed:` with nothing after it. The
   real cause was 60 `cl` errors on stdout, and a wrapper compiler was
   needed to read them. `tool_output_report` now renders both streams
   with a label for each, drops an empty stream, and names a silent
   command.

   Every call site that runs a C compiler or a linker reports both
   streams. Twelve call sites use `tool_output_report`: the Cranelift-AOT
   link, the C-AOT compile, the `aot` and `cemit` test hosts, the
   `offsetof` probe, the examples host gate, and the perf, size, and
   cross-language benchmarks. The CLI writes both streams to its own
   stderr, then returns the exit status. Two exceptions stay on stderr
   alone, because cargo writes its diagnostics to stderr on every
   platform: the runtime static library build in `codegen`, and the same
   build in the CLI. A report of a program run is not in scope here. It
   reports the run, not the toolchain.

## 12. P5 C-header binding vertical slice

The language's founding purpose (plan §4): express zero-copy C-ABI
interop. P5 proves it against a **neutral synthetic C header**
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

The ship tier is C emission (§11), where the platform C
compiler performs all boundary-struct argument marshaling and is correct
by construction on every ship target (arm64 iOS/Android and
`x86_64-unknown-linux-gnu`). The dev JIT must hand-build the C-ABI call, and passing
a boundary **struct by value** across a foreign call is ABI-specific. The
marshaler branches on the **target ABI**, not merely the architecture,
because x86-64 hosts split by OS: `x86_64-pc-windows-msvc` is Win64,
`x86_64-unknown-*` is SysV, and the two disagree on struct passing.

Implemented and verified:

- **AAPCS64 (arm64)**: an HFA/HVA is passed component-wise in float
  registers; any other composite of at most 16 bytes is passed in
  consecutive general registers as **eightbyte images** — the struct's
  bytes at their C offsets, one register per eightbyte — and a larger one
  is passed by reference to a caller copy (AAPCS64 B.4). *(Corrected
  2026-08-03 by §47, OBS-4: this rule read "its components as arguments",
  which is a different and wrong marshaling — it silently delivered wrong
  values for any by-value struct with two or more sub-eightbyte integer
  fields, since the callee reads a whole eightbyte where the caller had
  written one field. `{i64,i64}` and HFAs were unaffected, which is why
  it survived.)*
- **Win64 (`x86_64-pc-windows-msvc`)**: a struct whose total size is
  exactly 1, 2, 4, or 8 bytes is passed **by value in one integer
  register** — the whole struct as a single packed integer of that width,
  with no HFA/float-register special case and no multi-register packing;
  every other size is passed **by reference** to a caller copy. (A callback
  field expands to trampoline+binding = 16 bytes, so any struct carrying
  one is by-reference on Win64.)

Implemented 2026-08-09 and verified byte-exact on the x86-64-linux gate
(interop 13/13, golden 27/27, dev-JIT ≡ ship-C-AOT ≡ golden;
`specs/tracking/linux-portability.md`):

- **SysV (`x86_64-unknown-*`, System V AMD64)**: the struct is split into
  eightbytes and each eightbyte gets a class. An eightbyte whose bytes are
  all float (`f32`/`f64`) is class **SSE** and passes in the next SSE
  register as an `F64`/`F32` image; every other eightbyte is class
  **INTEGER** and passes in the next general register as an `I64` image,
  with sub-eightbyte fields packed at their C offsets. This is the AAPCS64
  eightbyte-image rule (§47) plus the per-eightbyte INTEGER/SSE split that
  AAPCS64 does not make. A struct of at most 16 bytes uses one or two
  eightbytes. A larger struct, or one with an unaligned field, is class
  **MEMORY**: an argument passes **on the stack by value** — a copy, not a
  pointer — and a return passes through a hidden pointer. SysV and AAPCS64
  diverge here: AAPCS64 passes a larger struct **by reference**, so the
  by-reference `Indirect` path stays AAPCS64/Win64 only.

On any host whose ABI is none of AAPCS64, Win64, or SysV, lowering a
foreign call that passes or returns a boundary struct by value stays a
**loud codegen error**, never a silent mis-marshal, since dev-JIT ≡ ship-C
equivalence is otherwise unverifiable there.

The corpus exercises the SysV **MEMORY argument** path directly — a 24-byte
`SubCallbackInfo` (`{ fn-ptr, void*, void* }`, in `a25`–`a90`) and a
`{ i64, i64, i64 }` triple (`a126`) are each passed by value — so that path
is **implemented**, not staged: the dev JIT builds the struct in a caller
slot and passes it by value on the stack (Cranelift `StructArgument`),
never the by-reference `Indirect` path. What stays a loud error on SysV is
a struct **return in SSE-class registers** (a float or HFA return in
XMM0/XMM1): the dev JIT models no float return register on **any** ABI —
the AAPCS64 and Win64 HFA-return cases are the same loud error — so this is
a shared, accepted follow-up, not a Linux-specific gap. An INTEGER-class
SysV return (RAX, then RDX) and a MEMORY return (hidden pointer, like the
other ABIs) are implemented.

Two SysV argument shapes stay a **loud error**, each a silent-mis-marshal
risk the differential gate cannot see (no corpus entry exercises them):

- **Argument register pressure.** psABI §3.2.3 step 5: when an aggregate's
  eightbytes do not all fit in the remaining argument registers, the whole
  aggregate reverts to the stack. The dev JIT pushes eightbytes as
  independent scalars, so it cannot split-then-revert; when it detects that
  the remaining GP/SSE registers cannot hold every eightbyte it raises a
  loud error rather than mis-marshal. The stack-revert path is a tracked
  follow-up. AAPCS64 has the same unmodeled revert and is the same
  follow-up.
- **An `f16` leaf in a register-class eightbyte.** The psABI classifies
  `_Float16` as SSE, but `f16` is storage-only here (§16.2) and lowers to
  `I16`, so the eightbyte would classify INTEGER. A register-class SysV
  aggregate with an `f16` field is a loud error; a MEMORY-class one (a byte
  copy) is unaffected. Full SSE classification of `f16` across both tiers
  is a tracked follow-up.

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
Windows-x64, and the SysV path on x86-64 Linux
(`specs/tracking/linux-portability.md`).

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
header and emits the mirror. *(Since 2026-07-30 this obligation is
discharged by `subscript bind` — same library entry, byte-identical
output, cli.md §10 — and the standalone binary is retired, cli.md
§11.)* The capability is demonstrated in-session
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

### 14.4a Callback bindings are interned by identity — Rev 2026-07-29

`bind_callback` allocated a fresh Context-held record on **every**
marshaled callback-info crossing and nothing ever swept it — not
`Context.free`, not `Context.collect`, only Context drop. The long-run
audit (`specs/tracking/long-run-audit.md`, finding 2) names the
consequence: a host that registers per frame grows one boxed record per
registration, without bound. The lifetime rule itself (§13.3: the binding
lives for the whole Context, because the C side holds the raw pointer and
the runtime cannot know when the host is done with it) is correct and
unchanged; the defect is that each registration paid for a new record.

**The fix is interning.** A boundary callback is non-escaping (C5), so
its function value is a non-capturing wrapper and `env` is always null —
`subscript_rt_cb_bind`'s own contract says so. A binding's identity is therefore
the tuple **(code, userdata1, userdata2)**. `bind_callback` returns the
existing record for a tuple it has seen, and allocates only for a new
one.

This converts the growth class rather than capping it: bindings become
**bounded by distinct (code, userdata) tuples used**, the same
bounded-by-distinct shape this project already accepts for the astral
code-point interns (§22.1) and the compiled-pattern cache
(`stdlib.md` §15.5a). The honest bound is stated the same way those two
state it: a program that registers a million distinct userdata objects
allocates a million bindings, and that is not a shape a real host loop
has.

Observable consequences, pre-registered:

1. Re-registering the same callback with the same userdata returns the
   **same binding pointer**; a C host may rely on pointer equality across
   re-registrations within one Context.
2. Deferred fires through a re-registered binding behave exactly as
   before — the record the C side stored at first registration *is* the
   record the pump reads.
3. No golden moves: no corpus entry or example observes binding identity
   or count today.

Exit criteria: (1) a runtime unit test registers one tuple twice and
asserts one record and pointer equality, and registers a second tuple and
asserts a second record; (2) a probe-style measurement (the
`dev-retention-probe` pattern) shows per-frame re-registration at zero
growth per frame; (3) standing gate green, no golden moved.

### 14.4b Registered callback userdata: rooted, checked at fire, advised at free — Rev 2026-07-30

Owner decision. Use-after-free through callback userdata — register,
release the object, the host fires later — is the natural failure of
the callback model, and the general freed-handle diagnostics
(§8.1a-1..3) are mis-shaped for it: their cost is proportional to
*everything freed* across a window that is exactly as long as the
host pleases. The binding records already hold the userdata pointers
(§14.4a), so this pattern gets three targeted mechanisms whose
standing memory cost is zero.

**(C) Registered userdata is rooted.** `Context.collect`'s mark phase
walks the live binding records and treats each userdata slot that is
a live allocation as a root; a slot that is not one (null, freed) is
skipped safely. No standing root table exists — the roots are derived
from the records at mark time. Consequence, both tiers: an object
registered as callback userdata survives collection and is released
only by explicit `Context.free` or Context release. Q13's "userdata
must outlive the registration" becomes a guarantee on the collect
side. Superseded registrations keep their records (§14.4a interning),
so they keep rooting their old userdata; the retention bound is
distinct registrations — the already-accepted intern class. Replacing
a registration does not release the old userdata; `Context.free`
does.

**(A) The trampoline checks liveness at fire.** Before a fired
callback enters script code, each non-null userdata slot is checked,
in order: in the freed-handle dead set (diagnostics mode on) — trap,
with the freed-allocation information; otherwise not a live
allocation — trap, best-effort in exactly §8.1a-2's class (certain
while the address is unallocated, undefined once reused). The trap
kind is a new stable kind for this pattern; message and position pin
in the corpus. Cost per fire: O(1) lookups against maps that already
exist; nothing is retained for this check.

**(B) Freeing registered userdata is advised.** A new optional
observer, `subscript_rt_ctx_set_diagnostics_observer(ctx, observer,
userdata)`, following the §18.2/§18.2f observer rules verbatim
(observation only, never re-entered, cleared by null). Its first
advisory: an explicit `Context.free` whose address is a userdata slot
of a live binding reports (advisory kind, `pos_id`, message) before
the release proceeds. It cannot be a trap: freeing userdata the host
will never fire again is legal, and the runtime cannot know
(§14.4a's reason). Observer unset — the default — skips the check
entirely; nothing changes for any existing program.

**(B2) Binding growth is advised — Rev 2026-07-30.** The frame-shaped
misuse — a per-frame entry registering with freshly allocated
userdata on every call — is invisible to static analysis (the loop
lives in the host), so it is observed dynamically. A host-set
threshold, `subscript_rt_ctx_set_binding_count_advisory(ctx,
threshold)`, default `UINT64_MAX`, literal semantics (0 = advise on
the first record; no sentinels — the §8.1a-3 convention): whenever a
**new** binding record is interned and the record count is at or
above the threshold, the diagnostics observer receives advisory kind
2 (`SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_BINDING_COUNT`), `pos_id` 0,
message carrying the count and threshold. Re-registering an existing
identity interns no record (§14.4a) and never advises — an advisory
therefore always signals real growth. No observer or default
threshold: the check is one comparison at intern time, nothing
retained. The static half of the same concern is `warnings.md` W003.

**Golden audit, pre-registered (2026-07-30).** No committed corpus
entry, gate program, or example both registers a sink and collects,
so (C) moves no committed golden. If implementation moves one, stop
and report; a moved golden is a finding, not an update.

**Corpus.** One new interop accept entry: register with userdata,
drop the script references, `Context.collect()`, pump — the callback
reads its userdata fields and the output is a committed golden
(defined behavior only under (C); this entry is the rooting proof).
One new interop trap entry in t22/t23's gating class: register, free
the userdata, pump — the fire-time trap's kind, message, and position
pin (A).

**Exit criteria (pre-registered).**

1. The rooting accept entry runs byte-identical under both tiers.
2. The fire-time trap entry pins (kind, message, position) under
   both tiers with diagnostics mode on, t22/t23's class; mode-off
   behavior is best-effort and not gated.
3. (B): a runtime unit test and an FFI test observe the advisory on
   free-of-registered-userdata; with the observer unset the full gate
   is green and byte-unchanged, which is the zero-cost proof.
4. A unit test asserts registered userdata survives collect
   (accounting), and that a freed slot is skipped at mark without
   fault.
5. No committed golden moves; `tsc` gate green with the new entries.

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
operation lowers to an opaque `subscript_rt_math_*`/`subscript_rt_date_*` call on
both tiers (never a direct libm emission — clang constant-folds libm at
`-O2`, a cross-tier divergence hazard, `stdlib.md` §0.2). Gate:
`stdlib.md` §5.

## 16. P14 — narrow numerics (`i8`/`u8`/`i16`/`u16`/`f16`)

Owner decision 2026-07-25. Language rules: `collisions.md` Q23 (which
extends C3/C4/Q18). This is a type-system extension, not a stdlib
phase — no new library surface, five new sized types.

**Why now.** `bindgen`'s scalar map (`bindgen/src/emit.rs::lang_scalar`)
has no entry for `uint8_t`/`uint16_t`/`char`/`short`, and the emitter
fails loud on an unmapped construct, so **a production header with a
single byte field cannot be bound at all** — the same class of blocker
as the tracked two-level flag aliases. `f16` additionally unblocks the
GPU buffer formats mobile shaders consume (half-precision vertex
attributes, uniform blocks); the script builds the buffer, the device
does the half-precision math.

### 16.1 Scope

- Five ambient aliases in `prelude/lang.d.ts`: `i8`, `u8`, `i16`,
  `u16`, `f16` — aliases of `number`, exactly as C3's existing six.
- Layout: 1/1, 1/1, 2/2, 2/2, 2/2 (size/align), verified by the §12.3
  `offsetof` proof against the platform C compiler, not asserted.
- Both tiers: dev JIT (`codegen/src/lower/`) and ship C
  (`codegen/src/cemit.rs`) — one set of semantics, proven by the
  standing gate (§11).
- `T[]` of a narrow type is contiguous and zero-copy across the C
  boundary, exactly as the existing primitive slices (a31).
- `bindgen`: `int8_t`/`uint8_t`/`char`/`signed char`/`unsigned char`/
  `int16_t`/`uint16_t`/`short`/`unsigned short` map to the new integer
  types. **`char` signedness is platform-dependent**; map plain `char`
  only where the target's signedness is known, else keep failing loud
  rather than guessing.

### 16.2 `f16` is storage-only

Per Q23: `f16` declares fields, elements and boundary parameters and
converts with `as`; **arithmetic on `f16` operands is S014**. The
conversion (`f16`↔`f32`/`f64`) is one runtime implementation behind an
opaque `subscript_rt_*` symbol on both tiers — never an emitted compiler
builtin and never a direct `_Float16`/`__fp16` operation, for the
§11/`stdlib.md` §0.2 reason: the C tier's `_Float16` rounds in half
precision while `__fp16` promotes to `f32`, so an emitted half
operation is a silent dev-JIT ≠ ship-C divergence waiting to happen.
Conversion semantics: IEEE 754 binary16, round-to-nearest-even;
overflow to `±Infinity`; subnormals preserved; `NaN` preserved.

The C boundary type for `f16` is fixed by the mirror, not inferred:
`bindgen` maps a half-width float field to `f16` only for the
spellings whose in-memory representation is unambiguously binary16
(`_Float16`, `__fp16`, and a `typedef`ed 16-bit float); anything else
fails loud.

### 16.3 Gate (pre-registered)

1. Standing gate (§11) byte-exact on every entry, both tiers,
   including the new corpus entries.
2. `offsetof` layout proof (§12.3) green for structs mixing the narrow
   types with the existing ones — padding reproduced exactly.
3. `tsc -p tsconfig.json` zero errors, unchanged config.
4. Reject entries at pinned S-codes: bare `number` unchanged (C3);
   out-of-range narrow literals (C4); mixed-width arithmetic and
   bitwise without `as` (C3/Q18); **`f16` arithmetic (S014, Q23)**.
5. `f16` conversion round-trips pinned in a committed golden across
   the interesting cases: representable value, value rounded on
   narrowing, overflow to `Infinity`, subnormal, `NaN`, `-0`.
6. A production-shaped C header carrying `uint8_t`/`uint16_t`/`f16`
   fields binds through `bindgen` and passes its `offsetof` proof —
   the blocker this phase exists to remove is demonstrably removed.
7. Benchmarks (`benchmarks.md`): no ship-row regression.

## 17. P16 — generated API reference

Owner decision 2026-07-25. The language's accepted surface is decided
in `collisions.md` (Q-register) and `stdlib.md`, but those are prose:
they are written by hand and can drift from the checker. That is not
hypothetical — the P12 review found a Q25 entry recording a divergence
that did not exist in the implementation, goldens or tests
(`specs/tracking/p9-stdlib.md`, P12 CRITICAL 1). This phase makes the
reference **derived** instead of written, so drift is impossible in the
generated part and demonstrable in the rest.

It is also the answer to the reader's real question. A TypeScript
developer does not need "what does subscript have"; they need **"which
of what I already know still works, and where does it behave
differently"** — because `tsconfig.json` loads the ES2022 lib (§0.1 of
`stdlib.md`), so the editor completes members this language rejects.
The generated reference is where that gap is stated.

### 17.1 Source of truth

The generator reads the **checker's own tables** — the ambient surface
(`compiler/src/ambient.rs`) and the per-type member tables the checker
consults — never the specs. Whatever the compiler accepts is what the
document says it accepts. Where a table needs an entry the generator
cannot infer (a human-readable summary line), the entry lives beside
the table in the checker, not in a parallel file.

### 17.2 Output

One generated document (Markdown; path the implementer's choice under
a generated-docs directory), carrying a do-not-edit header naming the
generator, with three parts:

1. **Accepted surface** — every ambient function, namespace member and
   type member the checker accepts, with its **subscript** signature
   (sized types, not `number`), grouped by receiver/namespace.
2. **Rejected surface** — every member the checker explicitly rejects,
   with its **S-code**, the **Q-rule** that rejects it, and the
   replacement where the contract names one (`findIndex` for `find`,
   `getOr` for a scalar `get`, `Number.isNaN` for the global). This is
   the part no hand-written document keeps current.
3. **Divergences from ECMA** — members this language accepts but whose
   *result* differs from JS, each naming the Q entry that records it.

### 17.3 A recorded divergence must be demonstrable

Every entry in part 3 carries an **executable witness**: a program
fragment plus the two results (this language's, and JS's). A test runs
each witness through the language and through `node` (already present
for the `tsc` gate) and **fails if they agree** — a divergence that
cannot be demonstrated is a spec error, and that is exactly the error
P12 shipped into `collisions.md`.

This checks one direction only. Divergences that exist but are *not*
recorded are found by adversarial sweeps in a Phase Review, not by a
standing test; the reference states which surfaces have been swept and
when, so an unswept area is visible rather than implied to be clean.

### 17.4 Regeneration is the gate

Running the generator on the current checker reproduces the committed
document **byte-for-byte**; a drift fails the test, exactly as
`bindgen`'s mirror does (§12.2). The document is never hand-edited —
CLAUDE.md core principle 6. A member added to or removed from the
checker without regenerating is therefore a build failure, not a stale
paragraph.

### 17.5 Gate (pre-registered)

Byte-identical regeneration test green; every accepted member in the
document is one the checker accepts and every rejected member is one it
rejects, asserted by construction rather than by review; every
divergence witness demonstrably diverges from `node`; the reject
S-codes in the document match the reject corpus's pinned codes; `tsc`
and the standing gate unaffected (this phase adds no language surface);
benchmarks not required (no runtime change).

### 17.6 Out of scope

The editor's over-promise — `tsserver` completing `Math.imul`,
`arr.find`, `str.substring` because the ES2022 lib declares them — is
**not** addressed here. Narrowing what the editor offers means shipping
a reduced `lib` for authoring while the `tsc` gate keeps the stock one
(§0.1), which is a separate design question. This phase makes the gap
*documented*; closing it is later work.

## 18. The host Context C API, and the trap observer

Owner decision 2026-07-26. **This section exists partly to close a
gap**: the `subscript_rt_ctx_*` surface is what an embedding host actually
calls, and it had no contract anywhere under `specs/` — it existed only
in `runtime/src/ffi.rs`. A host-facing ABI with no written contract is
the one surface where drift is least acceptable, since the host is
outside this repository and cannot be fixed by a commit here.

### 18.1 The existing surface, contracted retroactively

```c
void            subscript_rt_ctx_release(subscript_rt_context*);
const uint8_t*  subscript_rt_ctx_stdout(const subscript_rt_context*, uint64_t* len);
void            subscript_rt_ctx_seed_random(subscript_rt_context*, uint64_t seed);
void            subscript_rt_ctx_set_now(subscript_rt_context*, int64_t ms);
uint32_t        subscript_rt_ctx_trap_kind(const subscript_rt_context*);
uint32_t        subscript_rt_ctx_trap_pos_id(const subscript_rt_context*);
const uint8_t*  subscript_rt_ctx_trap_message(const subscript_rt_context*, uint64_t* len);
```

`seed_random` (`stdlib.md` §2) and `set_now` (§3) pin the two
nondeterministic inputs so tests and replays reproduce. The three
`trap_*` accessors are **post-hoc**: after a run returns, the host
reads the fault that stopped it. `Context::trap` records the **first**
trap and ignores later ones, so what the host reads is the
originating fault, not whatever was last seen while unwinding.

`pos_id` is an index into the compiler's position table; resolving it
to a TypeScript position needs that table, which is a separate
artifact from the Context.

### 18.2 The trap observer — observation only

```c
typedef void (*subscript_rt_trap_observer)(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len);

void subscript_rt_ctx_set_trap_observer(
    subscript_rt_context*, subscript_rt_trap_observer observer, void* userdata);
```

Called at the moment a trap is recorded, **before** the unwind, on the
trapping thread. Passing a null observer clears it.

**What it adds over §18.1**, honestly and only this: the host learns
while *its own* state is still current — which frame, tick, or entity
it was processing. Once the run has returned, that context is gone and
the post-hoc accessors cannot recover it. Secondarily, the host stops
having to poll the trap flag after every call into script.

**Observation-only is enforced by shape, not by documentation:**

- The observer returns `void`. There is no encoding for "continue", so
  C6's rule that trapping is not catchable survives by construction
  rather than by promise. This is not a recovery mechanism and no
  later revision may make it one without revisiting C6.
- It is handed **no Context pointer**. *(An earlier revision of this
  section said this made re-entry "structurally impossible". That was
  an overclaim, corrected 2026-07-26 after the implementer pointed it
  out: `userdata` can carry a Context pointer, so the signature
  withholds the means without preventing the act. Calling through a
  smuggled pointer is undefined behaviour, not a rule violation — see
  §18.2a — and the honest statement is that the shape removes the
  obvious path, not every path.)*
- It fires **at most once per run** — `Context::trap` is first-wins.
- It must not call back into script. The Context is trapped; re-entry
  is undefined and the host is responsible for not attempting it.
- It cannot change script-visible output, so §0.3 determinism and the
  golden corpus are unaffected. The observer receives copies and no
  mutable Context handle.

### 18.2a Exactly when it fires, and what is true at that moment

The sequence, from fault to the host getting control back:

1. A runtime function, or an emitted check calling `subscript_rt_trap`,
   detects the fault.
2. `Context::trap(kind, message, pos_id)` stores a `TrapRecord` **if
   none is stored yet**, and sets `trap_flag = 1` **unconditionally**.
3. **The observer fires here.**
4. The runtime function returns a placeholder to generated code.
5. Generated code checks the trap flag after every **fault-capable**
   call and, seeing it set, pops its shadow frame and returns early —
   zero for a non-`void` return, `1` for a generator. *(Corrected
   2026-07-26: this said "after every script call", contradicting
   §18.2b two sections later. The dev tier implements fault-capable;
   the ship tier implements neither, which is P19 — §19.)*
6. Every caller repeats step 5, up every live frame.
7. The tier entry reads the record and reports `RunError::Trap`.

At step 3, therefore:

- **Every script frame is still live.** Nothing has unwound; the host
  call site that entered script is still below on the stack.
- **The record is already stored and the flag already set.** The
  observer sees exactly what the §18.1 accessors will report later,
  not a provisional state.
- **The shadow stack still holds every frame**, since `shadow_pop`
  happens during the unwind at step 5.

**The call must sit inside the `trap.is_none()` branch, not beside
it**, and the difference is observable. Runtime functions stay callable
on the unwind path and may detect further faults; those later
`trap()` calls are ignored for the record but still set the flag.
Firing outside the branch would report them too, contradicting
"at most once per run" and handing the host arguments that do not
match the record.

**Message lifetime.** The observer receives a pointer into the stored
record, which lives on the Context until it is released or reset — not
a borrow valid only for the callback. The host is not obliged to copy.

**Re-entrancy is a memory-safety requirement, not a convention.** The
observer runs inside `Context::trap`, which holds `&mut self`. An
observer that calls any `subscript_rt_*` function taking the Context creates
an aliasing violation across the FFI boundary — undefined behaviour,
not merely a semantic error. The C header must say so.

**Cost when no observer is registered:** one null check *per trap*,
not per call. Traps are rare; this is not a hot path.

**Implementation.** All trap sites — 70 across the runtime at the time
of writing — funnel through the single `Context::trap`, so the
observer has exactly one call site. Nothing in generated code changes,
and neither tier's lowering is touched. This is also why §18.4's
both-tier test is not checking that the hook mechanism differs between
tiers — it cannot — but that the two tiers **agree on which fault is
the originating one**.

### 18.2b `subscript_rt_ctx_clear_trap` — making a trapped Context callable again

```c
int subscript_rt_ctx_clear_trap(subscript_rt_context*);   /* 1 = cleared, 0 = refused */
```

C6 says the host decides what happens after a trap, and until now it
could not decide "continue": `Context::clear_trap` existed with the
right semantics and a proving unit test, but was **never exposed over
the C ABI** — its only production caller was the reload session. A
host could therefore only release a trapped Context and rebuild.

**Precondition, checked by the function, not left to the caller.**
Clearing is legal only at a host↔script boundary — no generated code
on the stack. Clearing while a script frame is live would resume a run
that has already given up. The function **returns 0 and does nothing**
if the precondition fails; it must not be a documented obligation,
because the one place a host would most naturally try it — inside the
trap observer — is exactly the illegal case (§18.2a: every script
frame is still live when the observer fires).

**Two guards are required, because `script_depth` alone is inert.**
*(Corrected 2026-07-26. This section originally named `script_depth ==
0` as the whole check. The implementer found that nothing maintains
`script_depth` outside the reload session — ordinary `run_jit` and AOT
host entries never touch it — so for the deployment shape that matters
it reads 0 always, the guard always passes, and it passes **from
inside the observer**, which is the exact case it exists to refuse. A
guard that is inert in production is worse than none, because it reads
as protection.)*

1. **An observer-active flag on the Context**, set for the duration of
   the observer call. `clear_trap` refuses while it is set. This is
   trivially correct and addresses the actual hazard directly.

   It is a **backstop, not a supported call**. Reaching
   `subscript_rt_ctx_clear_trap` from inside an observer already requires a
   Context pointer, and §18.2a makes calling any `subscript_rt_*` through
   one from there undefined behaviour. The flag turns the most likely
   such attempt into a defined refusal instead of a resumed run; it
   does not make observer re-entry a defined API, and nothing else
   about §18.2a's prohibition is relaxed.
2. **`script_depth` maintained for real**, which needs the host
   enter/exit API of §18.1a. Until that exists the depth check is
   retained but is not load-bearing, and this section says so rather
   than implying coverage it does not have.

### 18.1a Host enter/exit — making `script_depth` real

```c
void subscript_rt_ctx_enter_script(subscript_rt_context*);
void subscript_rt_ctx_exit_script(subscript_rt_context*);
```

A host brackets each call into an exported function with these. They
maintain `Context::script_depth`, which is what makes "no generated
code on the stack" a checkable condition rather than a comment. The
reload session already maintains the depth internally; this exposes
the same discipline to an embedding host.

They are **not** optional for a host that calls `clear_trap`: without
them the depth is always 0 and §18.2b's first guard is the only real
one. A host that never clears may ignore them.

### 18.1b The host C header

**`runtime/include/subscript_runtime.h`**, generated. *(Until
2026-07-26 there was none: the runtime's declarations were embedded ad
hoc in `AOT_ENTRY_C` and the emitted-C preamble, so a host outside this
repository had nothing to include — in a language whose fourth
invariant is that host interop crosses a C ABI only. Found while
implementing §18.2 and closed in the same phase; the AOT entry, the
emitted-C preamble and the benchmark entry all consume it now instead
of repeating declarations.)*

It is a single generated header covering the `subscript_rt_ctx_*`
surface (§18.1, §18.1a, §18.2, §18.2b, §18.2d) and the exported-entry
convention, **generated from the Rust declarations rather than
hand-written**, on the `bindgen` mirror's principle (§12.2) and
CLAUDE.md core principle 6 — generated code is never hand-edited, fix
the generator. A hand-kept header would drift from the ABI it claims
to describe, which is the failure this section was written to close.

**It clears reporting state and nothing else.** Live allocations stay
live, deleted ones stay deleted, the reload epoch survives, the stdout
sink survives. So:

> **Clearing makes the Context callable again. It does not roll
> anything back.** There is no transaction. A run that trapped
> mid-`update` leaves script data exactly as it was at the fault —
> an entity may be half-written.

The host's three coherent choices, in increasing cost: accept the
damaged state and continue; continue but detach the failing subsystem
(which is what the observer's frame-current context is *for*); or
release the Context and rebuild from `subscript_init`, which is the only one
that restores consistency.

**Not clearing is a silent failure mode worth naming.** The trap flag
stays set, and generated code checks it after every fault-capable
call — so the *next* exported call unwinds immediately, executing
nothing. The host keeps running while the script is quietly dead.

### 18.2c How a host detects a trap at all

Exported functions return `void` or their declared type, **never a
status**, and a trapped call returns the **zeroed** value for a
non-`void` return (`emit_trap_return`). An `update(): i32` that
trapped hands the host `0`, indistinguishable from a legitimate `0`.

**The return value therefore cannot be used to detect a trap.** The
host tests `subscript_rt_ctx_trap_kind(ctx) != 0` — the accessor returns `0`
when no trap is pending and `TrapKind` starts at 1 — or registers an
observer (§18.2) and tests its own flag. This is stated because
getting it wrong is silent: the host reads a plausible zero and
carries on.

### 18.2d Memory accounting — `subscript_rt_ctx_live_*` / `subscript_rt_ctx_reserved_bytes`

```c
uint64_t subscript_rt_ctx_live_allocations(const subscript_rt_context*);
uint64_t subscript_rt_ctx_live_bytes(const subscript_rt_context*);
uint64_t subscript_rt_ctx_reserved_bytes(const subscript_rt_context*);
```

Owner decision 2026-07-26, and this closes a larger gap than §18.2's.
**Invariant 2 — no implicit GC — makes explicit lifetime management
the memory model's centre, and the host had no way to measure whether
it was working.** A script that forgets `Context.free` and leaks a
little every frame is invisible from outside: `Context::live_count`
and `is_live` existed but were Rust-side only. The P15 review found a
container retaining 8.4 MB after churn; a benchmark caught it, and a
production host embedding the same build could not have.

- `live_allocations` — count of live allocations. **Tier-independent**:
  the same program has the same number of live objects on both tiers,
  so a host may compare dev and ship figures and a difference is a
  defect.
- `live_bytes` — payload bytes of those allocations.
- `reserved_bytes` — what the Context holds from the system: chunk
  capacity plus large allocations. This is the figure a memory budget
  is written against, and the one that moves when memory is freed to a
  free list but not returned.

**`live_bytes` and `reserved_bytes` are tier-dependent, and that is
not a defect.** The two tiers have different allocators — the ship
tier (§8.1b) bump-allocates fixed-size blocks in size-class chunks and
therefore rounds a payload up **when it fits a size class** — an
allocation above `LARGEST_BLOCK` is an individual system allocation of
its exact size — while the dev tier allocates exact payloads
throughout. A host comparing byte figures across tiers will see them
differ; only the **count** is comparable. Stating this is the point of
the entry, because a host that assumed otherwise would chase a
non-bug.

Measured on one program (3 allocations, 1 delete), reported as
`(live_allocations, live_bytes, reserved_bytes)`:

```
dev  = (2, 8, 60)   # measured before §8.1a-1; with freed-handle
                    # diagnostics off, the deleted allocation's layout is
                    # no longer reserved
ship = (2, 32, 65536)
```

The count agrees; neither byte figure does.

**Cost.** `reserved_bytes` is O(chunks + live large allocations) on the
ship tier and walks the retained allocation records on the dev tier when
freed-handle diagnostics are on (§8.1a-1) —
cheap, but not O(1). `live_allocations` and `live_bytes` walk live
blocks on the ship tier and are **O(live blocks)** — they are diagnostics, not per-frame counters. The contract
deliberately does **not** add running counters maintained in
`alloc`/`delete`: that would make the figures O(1) at the price of an
invariant that must stay correct across delete, chunk reuse and
`Context.collect()`, and a memory statistic that can itself drift is worse
than one that is slow.

Read-only: none of the three can change script-visible output, so
§0.3 determinism and the golden corpus are unaffected. A host that
makes decisions from them introduces its own nondeterminism, exactly
as reading a clock does; that is the host's to own.

**Gate.** Across both tiers, `live_allocations` agrees for the same
program at the same point. A program that allocates N objects and
deletes M reports N−M. After a trapped run followed by
`subscript_rt_ctx_clear_trap`, the figures are unchanged by the clear —
which is what makes §18.2b's "clearing rolls nothing back" claim
**host-verifiable** rather than only provable inside the runtime's own
tests. `reserved_bytes` never decreases across a `delete` alone
(memory returns to a free list, not to the system), which is the
property that would have surfaced the P15 retention from outside.

### 18.2e Per-allocation attribution — superseded by §21.2

**Contracted in §21.2**, folded into P21 with allocator fault
injection because both touch `Context::alloc` and the allocation
header. The finding that motivated it is kept below; the design lives
in §21.2.

The question was whether a live-allocation figure could name the
script variable behind it. **It cannot, and the obstacle is not
cost**: an allocation is not bound to a variable — values move between
variables and into fields and arrays — so "the variable naming this
allocation" is not well defined, and a leaked allocation is usually
reachable from no named variable at all, which is why it leaked.

What *is* available is better suited to the purpose. Each allocation
carries a 16-byte header holding a state word and a `class_id`, with
**four bytes unused**. And `alloc(size, class_id, pos_id)` already
receives `pos_id` — the compiler position-table index for the
allocation **site** — and currently discards it, using it only if the
allocation itself fails. Storing it costs no space and one `u32` store.

An allocation site discriminates where a variable name would not: a
loop allocating ten thousand times has one site and no useful name.

Sketch, to be settled when this is contracted:

```c
typedef void (*subscript_rt_alloc_visitor)(void* userdata, uint32_t class_id,
                                     uint32_t pos_id, uint64_t payload_bytes);
uint64_t subscript_rt_ctx_visit_live_allocations(
    const subscript_rt_context*, subscript_rt_alloc_visitor, void* userdata);
```

Two things to settle then, both of which the sketch does not answer:

- **The `class_id` and position tables are the compiler's, not the
  Context's.** §18.1 already records this for a trap's `pos_id`. A
  host cannot turn either id into a name without an artifact this
  repository does not currently emit — the same gap as §18.1b's
  missing header, and it should be closed the same way.
- **The extra store lands in the ship tier's arena path (§8.1b)**,
  which is performance-sensitive; that tier's justification is that
  emitted C is close to hand-written C. One `u32` store per allocation
  is expected to be negligible, but it is to be **measured**, not
  asserted.

### 18.2f The print observer — streaming output without retention

**The stdout sink is cumulative and a C host cannot drain it.** `print`
appends to the Context sink; `subscript_rt_ctx_stdout` is a `const` read
returning a pointer into it; the draining accessor exists only on the
Rust surface, where the in-repo runners call it once at run end. That is
correct for the gate — the golden comparison wants the whole run's bytes —
and unacceptable for a long-running host, which retains every byte its
script ever printed (`specs/tracking/long-run-audit.md`, finding 1).

**The fix is the trap observer's shape applied to print** (§18.2 is the
precedent, deliberately):

    typedef void (*subscript_rt_print_observer)(void* userdata,
                                          const uint8_t* line,
                                          uint64_t line_len);
    void subscript_rt_ctx_set_print_observer(subscript_rt_context* ctx,
                                       subscript_rt_print_observer observer,
                                       void* userdata);

- **When set, each `print` delivers the line to the observer and retains
  nothing.** The sink does not grow. The bytes passed are exactly the
  bytes the sink would have stored, minus nothing — determinism is a
  property of the bytes, not of where they land.
- **When unset — the default — behaviour is exactly today's**: the sink
  accumulates and `subscript_rt_ctx_stdout` reads it. The gate never sets an
  observer, so every golden stands.
- The observer is called during `print`, inside script execution, under
  the same constraint as the trap observer: it must not call back into
  any `subscript_rt_*` API taking this Context (§18.2's aliasing rule, stated
  once there and referenced here).
- `line`/`line_len` are the line **without** the trailing newline the
  sink stores; the observer decides its own framing. Valid only for the
  duration of the call, like the trap observer's message.
- Switching the observer mid-run is allowed (unlike the freed-handle
  diagnostics setting there is no allocation-order invariant); bytes
  printed while unset stay in the sink, bytes printed while set are not
  added to it. A host that mixes the modes owns the seam.

Exit criteria: (1) a runtime unit test proves set-mode delivers each line
and leaves the sink empty, unset-mode accumulates, and a set→unset switch
leaves earlier lines observed and later lines sunk; (2) an FFI test
exercises the C surface both ways; (3) the capstone or a host example
gains the observer so the pattern is taught, with its golden unchanged in
meaning; (4) standing gate green, no golden moved; (5) the generated host
header documents the aliasing constraint and the no-retention property.

### 18.3 Why there is no in-language observer

A script cannot observe its own traps, and this is deliberate rather
than unimplemented. C6 makes trapping uncatchable; **a script-visible
trap observer is the first half of an exception mechanism** — once a
program can see its own fault, the pressure to let it continue arrives
immediately, and the property that "a trap stops the run" stops being
something the rest of the contract can rely on. Q20 (no Invalid-Date),
Q25 (`toFixed` range), Q27 (`shift` on empty) and Q28 (`NaN` in
`stringify`, reading `value` when `ok` is false) all lean on it.
Script-visible state that depends on a fault would also be a
determinism hazard.

### 18.4 Gate

A host-side test registers an observer, runs a program that traps, and
asserts: the observer fired exactly once; `kind`, `pos_id` and
`message` equal what the §18.1 accessors report afterwards; the run
still unwound and the Context is still trapped; and a second trap in
the same run does not fire it again. The same test runs on **both
tiers** — the observer is runtime-side, so both must agree, and a
divergence here would mean the two tiers disagree about which fault is
the originating one. Clearing the observer with null is covered.

Determinism: a corpus entry's output is byte-identical with and
without an observer registered. This is the check that the shape
really is observation-only.

For `subscript_rt_ctx_clear_trap`: a host-side test traps, clears, and calls
script again, asserting the second call **executes** rather than
unwinding on a stale flag; asserts a clear attempted with a live script
frame returns 0 and leaves the trap pending; and asserts that live
allocations, the reload epoch and the stdout sink are unchanged across
the clear — the property that makes "no rollback" true rather than
merely claimed. Both tiers.

## 19. P19 — trap unwind parity (CRITICAL) — RESOLVED 2026-07-26

**Everything in §19.1–19.4 below is the state that was found, kept in
the past tense it was written in rather than rewritten, because the
evidence is the reason the fix took the shape it did.** §19.7 records
what landed. The one item still open is the emitted-C ratio in §19.7.

Found 2026-07-26 by a fresh no-context investigation, opened as its own
phase because it predates P13 and P18 and is larger than either.

### 19.1 What is wrong

**The two tiers execute different amounts of code between a fault and
the stop, and therefore produce different output.** Measured: of 19
trapping programs, **14 differ in stdout bytes** between dev-JIT and
ship-C-AOT. Trap tuples agree in every case; only what happens before
the stop differs.

The bound is not "one statement". It is **the end of the enclosing
function**:

- a loop containing the fault **runs to completion** (5 and 4
  iterations measured), and the statements after it execute;
- execution **enters another script function and completes its body**;
- an array is **pushed to twice after the fault**, its length going
  0 → 2 where the dev tier leaves it 0 — so **live Context state
  diverges**, not only stdout.

**And the continuation path corrupts memory.** An out-of-range write to
a 320-byte `@CStruct` array element goes through `subscript_arr_at`, which
records the trap and then returns `subscript_scratch` — `static unsigned char
subscript_scratch[256]` — and the caller writes 320 bytes into it.
AddressSanitizer reports `global-buffer-overflow`. The dev tier branches
to unwind before the address is computed and never reaches the store.
**This write happens only on the post-trap path.**

### 19.2 Why the gate did not catch it

Three independent reasons, each sufficient:

1. `corpus.md`'s determinism rule requires every accept program to
   terminate with deterministic output, so **a trapping program cannot
   be an accept entry** and never reaches the golden comparison.
2. `corpus.md` states, for the `trap/` category added the same day,
   that a trap entry's "observable result is the trap tuple, **not
   stdout**". The gate was told not to look.
3. **The API discards it**: `run_jit` drops the Context on the
   `RunError::Trap` path, and `run_c_aot` returns `run.stdout` only on
   success. The 25 trap-parity tests in `codegen/tests/cemit.rs` use
   these two functions and compare tuples alone. Comparing pre-trap
   output is not currently possible.

So the founding invariant — dev-JIT ≡ ship-C-AOT, byte-exact — fails
for a whole class of programs that the gate structurally excludes.

### 19.3 The rule, and which tier is right

**The dev tier is the reference.** `collisions.md` C6 says a fault
means "the Context stops", and the dev tier implements that: `guard()`
branches to unwind at the fault point, and `trap_check()` follows every
call that can leave the Context trapped, selected by the shared
predicates `ArrFn::can_trap()`, `StrFn::takes_pos_id()`,
`MapFn`/`SetFn::can_trap()`, `NumFn::takes_pos_id()`.

**The ship tier consults none of those predicates.** `cemit.rs` never
references `can_trap()`; it emits a check after script calls,
constructors, value-position `pop`, generator `.next()` and
`JsonResult.value`, and after nothing else — not after `Callee::Str`,
`Arr`, `Map`, `Set`, `Math`, `Num`, `Foreign`, not after
statement-position `push`/`pop`, allocation, `delete` or string
formatting, and not on a loop back-edge.

The fix is structural, not a longer list: **both tiers consult one
shared predicate.** A trap-capable operation added later must become
checked in both tiers by construction, which is the only way this stays
fixed.

### 19.4 Two contradictions in this document, both to be resolved here

- §18.2a step 5 says generated code checks the flag "after every
  **script call**". §18.2b says "after every **fault-capable call** —
  so the *next* exported call unwinds immediately, executing nothing".
  These disagree with each other; the dev tier implements the second,
  the ship tier neither. **The second is correct** and §18.2a is to be
  amended, not the reverse.
- C6's "the Context stops" is true of the dev tier and false of the
  ship tier. C6 is right; the ship tier is wrong.

### 19.5 Staging — the gate first

**Stage B (Red) — make the divergence visible.** Return the stdout sink
on the trap path from both `run_jit` and `run_c_aot`; allow a
`corpus/trap/` entry to carry an `.expected`; amend `corpus.md`'s
"tuple, not stdout" rule; change the 25 trap-parity tests to compare
`(tuple, stdout)`. Expected outcome: **a large number of failures.**
That is the point — this stage is not expected to be green.

**Stage A (Green) — make the tiers agree.**

1. **Read the trap flag inline.** The ship tier's checks currently call
   `subscript_rt_ctx_trap_kind`, an out-of-line `extern` that the link cannot
   inline (no LTO). Measured, 50M iterations, arm64 `-O2`: an inline
   load of the flag costs **≈0.56 ns** per check against **≈6.4 ns**
   for the call — about 11×. Do this **before** raising check density,
   or the density increase pays eleven times over.

   The flag is at Context offset 0, already contracted
   (`Context::trap_flag_offset`) with a test proving the offset, and
   the dev tier already loads it directly. This makes emitted C assume
   the Context layout, which is a **deliberate exception** to §18.1b's
   rule that the generated header is the sole expression of the ABI;
   the exception is recorded rather than left implicit.

2. **Share the predicate** (§19.3) so both tiers check the same set.

3. **Inline the checks inside the helper functions.**
   `subscript_sdiv_*`/`subscript_udiv_*` (16 of them) and `subscript_arr_at`/`subscript_fa_at`
   **cannot be fixed as functions** — a C function cannot make its
   caller return. The check must be expanded at the call site. **The
   `subscript_scratch` corruption closes only this way**; widening the buffer
   would relieve the symptom while leaving a store executing after a
   fault.

**Benchmarks are a gate item here, not a formality**, unlike the two
phases before it: this phase changes emitted code on the hottest paths
(division, indexing) and the ship tier's justification is that emitted
C measures 1.05× hand-written C. Re-run `perf-gate` and the
cross-language suite, and record the rows. A regression is a finding,
not an acceptable cost, without an explicit owner decision.

### 19.7 What landed

**Fully green**: `cargo test --offline` reports 559 passed, 0 failed;
`codegen/tests/cemit.rs` 68 passed. All 11 of stage B's intentional
failures are fixed with no `.expected` touched and no assertion
weakened — including the accept-corpus goldens, which is the check that
non-trapping behaviour did not move.

- **`Callee::can_trap()` in `compiler/src/hir.rs` is the shared
  policy for *calls*,** consumed by both lowerings, and the dev tier's
  behaviour is unchanged by the refactor.

  *(Corrected 2026-07-26 after the Phase Review. This said a
  trap-capable operation added later is checked in both tiers "by
  construction rather than by remembering". That is true of `Callee`
  variants and **false of everything else**: about ten non-call trap
  sites — integer div/rem, index read and write, `JsonResult.value`,
  narrowing casts, use-after-delete, stale-coroutine, and the
  allocation-bearing literals — remain hard-coded separately in each
  tier. Both of this phase's own CRITICALs were instances of that
  duplication failing.)*

  **Finishing the structural fix is possible but is not a small
  predicate.** A shared `TrapSite` classification plus a both-tier
  coverage test is ~0.5–1 day; a real explicit checked-operation IR is
  ~2–4 days and a few hundred lines. A boolean predicate alone cannot
  express what these sites need — the proof-based elision of a proven
  index, the *two* resolutions a compound assignment requires, the
  several trap points inside a template or array literal, and the
  position each guard reports. Recorded as the open item rather than
  claimed as done.
- **One inline Context-layout assumption**, `*(const uint32_t*)ctx` at
  a single site in `cemit.rs` — the §19.5 exception to §18.1b, kept to
  one place so it cannot spread.
- **`subscript_arr_at`, `subscript_fa_at` and `subscript_scratch` are gone**, their checks
  expanded at the call sites. ASan on the 320-byte `@CStruct` case:
  before, `global-buffer-overflow`, `WRITE of size 320`, exit 134;
  after, clean. A regression test pins it.
- **No loop back-edge polling.** The implementer checked the dev tier
  rather than assuming: it has no unconditional back-edge poll either,
  checking exact fault-capable operations instead. Ship-only polling
  was measurably slower and was removed. Both tiers now stop at the
  fault for the same reason.

**P19 did not regress emitted C — it made it 2.4× faster.** Measured as
a controlled pair, both runs by the orchestrator, same machine, same
session, `--warmup 12 --timed 15`, both valid under the ±20% rule:

| tree | emitted-C median | C baseline | ×C |
|---|---:|---:|---:|
| pre-P19 (`2fda4a9`, git worktree) | 14.75 ms | 4.04 ms | **3.65×** |
| post-P19 (`8f8a851`) | 6.08 ms | 3.97 ms | **1.53×** |

The C baselines agree to 1.7%, which is the control that makes the
comparison mean anything.

The result is the opposite of the expected direction. *(The mechanism
originally recorded here — that `subscript_arr_at` was "an out-of-line call
per array element access" — was **wrong**, and the Phase Review
measured it: `subscript_arr_at` was `static`, and clang inlined it. Corrected
below from the emitted assembly.)*

What the old shape cost per access was not a call but everything the
**fallback pointer** forced: a null compare and `csel` choosing
between the returned pointer and `subscript_scratch`, the global address of
`subscript_scratch` held live, a reachable cold-arm `bl _subscript_rt_array_ptr`,
and — because that call is reachable inside the loop — an 80-byte
frame with callee-saved spills and reloads. The loop body was 82
instructions with a full spill set. Expanding the check at the call
site makes the out-of-range arm return immediately, so the cold call
becomes a tail branch and the frame, the spills, the `csel` and the
scratch reference all disappear: 39 instructions, no spills. `a22` is
matrix propagation, so indexing is its inner loop.

In one line: the helper cost **leafness, register pressure and alias
freedom**, not a call. Adding trap checks made the emitted C faster
because expanding them removed all four.

**The gap to 1.05× is closed as a question: there is no regression to
fix, and 1.05× is not a target to return to.** Bisected on the same
machine, with the trap checks emitted into `a22` counted directly:

| tree | out-of-line checks | inline checks | ×C |
|---|---:|---:|---:|
| pre-P13 (`6b5189d`) | **0** | 0 | 1.87× |
| post-P13 (`f3e1d5a`) | 15 | 0 | 3.74× |
| pre-P19 (`2fda4a9`) | 15 | 0 | 3.65× |
| post-P19 (`8f8a851`) | 1 | 24 | **1.53×** |

**The emitted C had no trap check at all before P13.** P13 added the
checking that C6 requires after a script call and paid for it in the
out-of-line form — that is the 1.87× → 3.74× step, and it was the price
of correctness rather than a defect. P19 then fixed the form and
widened the coverage: **25 checks against P13's 15, and 2.4× faster**.

Post-P19 is also faster than the tree that had **no** checks, because
removing `subscript_arr_at`'s per-access call outweighed adding 25 trap
checks.

So **1.05× was measured on an emitter that did not do the checking the
language requires**, and comparing 1.53× against it compares two
different correctness levels. The §3 threshold of 1.50× is very nearly
met with correct semantics, which is the comparison that means
something. CLAUDE.md's citation of 1.05× as the evidence for choosing C
emission should be read as historical; the decision it supports is
unaffected, since emitted C is still ~15× faster than the Cranelift
AOT path it replaced.

Cross-language timings are **not reported**: the run matched all nine
checksums but was voided by the harness on C-subject spread (27–97%)
even at 200 warm-ups. Reporting them would violate §9.

### 19.6 Rejected: `setjmp`/`longjmp`

A real unwind in the ship tier would cost nothing per check. It is
rejected: it skips `shadow_pop`, corrupting the shadow stack that the
GC roots depend on, and breaks the "generated code never unwinds"
property that `codegen/src/jit.rs`'s SAFETY comments rely on.

## 20. P20 — the trap-site IR

Owner decision 2026-07-26, after P19's Phase Review. P19 made the two
tiers agree on *call* trap sites by giving them one shared predicate,
`Callee::can_trap()`. About ten **non-call** trap sites were left as
hard-coded policy in each lowering — and **both of P19's own CRITICALs
were instances of that duplication failing**, so this is a demonstrated
hazard, not a tidiness argument.

### 20.1 The property to buy

**It must be impossible to add a trap-capable operation that one tier
checks and the other does not.**

"Impossible", not "tested". P19's §19.7 originally claimed the
`Callee` predicate delivered this "by construction"; the review showed
the claim was broader than the mechanism. The distinction matters:

- a coverage test is **remembering** — it catches the omission only if
  someone extends the test alongside the operation;
- an **exhaustive match over an explicit IR node** is *construction* —
  adding a variant fails to compile in both lowerings, before any test
  runs.

**P20 delivers the second.** A `TrapSite` that either lowering does not
handle is a build error. If the design ends up leaning on a test to
notice an omission, the phase has not met its exit criterion.

**Scope of the guarantee, measured by the Phase Review** (2026-07-26):
it covers a new **variant**, and initially did not cover a new **site
of an existing variant**. Both lowerings select with
`sites.iter().find(|site| matches!(site, TrapSite::X { .. }))`, so an
extra site appended to some operation's sequence compiled in both
tiers and was **silently dropped** by whichever one did not look for
it — leaving §20.5's criterion 2 held by the differential gate rather
than by construction, which is the distinction this section exists to
buy. Only `eval_array_lit` and `eval_template` asserted full
consumption. Closing that is part of the phase; the guarantee is not
"exhaustive match" but **"every derived site is consumed"**, and the
first is only half of the second.

### 20.2 What a trap site has to carry

A boolean cannot express these; each was found in the P19 work.

- **Position.** Each guard reports its own `pos_id`. P19's CRITICAL 2
  diverged in the *trap tuple*, not only in stdout, because the two
  tiers resolved different sites and therefore reported different
  positions.
- **Elision.** §10a lets a proven-in-range index skip its check. The
  set of sites is therefore **decided once, in HIR**
  (`compiler/src/trap_sites.rs`), so both tiers inherit the same
  decision — including §10a's elision of a proven-in-range index, which
  each lowering used to re-derive. That re-derivation was the root of
  the duplication. **This was the substantive part of the phase**: the
  checks a program carries are now a property of the HIR rather than of
  either backend.
- **Multiplicity.** A compound assignment to an array element needs
  **two** resolutions — check-and-read before the RHS, check-and-write
  after — and P19's CRITICAL 2 was exactly one of them being dropped.
  A template literal or an array literal carries several sites. One
  operation therefore maps to a *sequence* of sites, not to a flag.
- **Operands.** A site owns the values its guard tests. P19's
  CRITICAL 1 was an operand string interpolated twice, calling a
  call-valued divisor twice — the guard must hold a materialized
  operand, not a re-emittable expression.

### 20.3 Sites in scope

The non-call sites P19 left in two places: integer div/rem, index read,
index write, `JsonResult.value`, narrowing `as` (null and class
mismatch), the `Context.free` lifetime checks, stale-coroutine,
allocation failure, and the allocation-bearing literals — string
literal, `str_concat`, `fmt_*`, array literal.

*(Corrected 2026-07-26 by the Red stage, which measured rather than
assumed.)* Four of those descriptions were wrong:

- **Narrowing `as` was not "in two places" — the C emitter had no guard
  at all**, only a plain cast (`cemit.rs`, `eval_cast`), whose comment
  claims the path "is not exercised by the run set". C3 says `x as C`
  "traps on `null` or on class mismatch, **in both tiers**". It does
  not, and the Red stage reached it from a corpus program through the
  ambient host-boundary callback types — which is where C7 admits
  `object | null` in the first place, so the path is reachable **by
  design**, not by accident. **This is elevated to the first item of
  stage Green**: it is not a reporting difference but a type-safety
  hole — the ship tier hands back a class-typed reference to an object
  that is not that class, unchecked. P19's review had assessed it as
  unreachable; that assessment was about the run set, not the language.
- **Stale-coroutine exists only under the JIT's reload mode.** The C
  tier has no body-swap mode, so there is no tuple to compare.
- **Allocation failure is not corpus-reachable**: there is no
  source-level memory quota and no allocator fault injection, and
  exhausting real memory is neither safe nor deterministic under
  overcommit. It stays in scope for the IR but cannot be corpus-tested;
  a fault-injection hook is the follow-up if it is wanted.
- **The allocation-bearing list is incomplete.** It omits reference
  class `new` and its constructor unwind, the checker-generated JSON
  `RawNew`, and generator-frame creation — all non-call HIR paths with
  hard-coded checks. Add them.

**A C-only fault point, found by the Red stage and in scope:** every
**non-empty** template in the C emitter emits a checked empty-string
allocation the JIT does not, so the two tiers differ in both the number
and the placement of allocation fault points inside a template. The IR
must give a template the same site sequence on both tiers.

`InvalidDelete` — releasing a pointer the Context does not own — is a
runtime fault this section did not name and which no valid program can
construct today. Recorded so the omission is deliberate.

`Callee::can_trap()` was already shared; it is **folded into
`Expr::trap_sites()`**, so there is one answer to "what can fault
here", not two.

Where a site is **deliberately** checked on one tier only, that must be
representable and stated rather than implicit — Q6's use-after-delete
carve-out (§8.1b) is the existing case, and the review confirmed it is
contracted rather than accidental.

### 20.4 Two pre-existing defects, in scope

Both were found by P19's review, both reproduce before P19, and both
live in the index/compound-assign path this phase restructures.
Fixing them elsewhere would mean touching that path twice.

- `xs[i] += "s"` on a `string[]` emits invalid C — `*p = *p + v` on
  `void*`; there is no `Type::Str` arm.
- `(xs[i] += v)` in expression position fails the ship tier with an
  internal lowering error where the dev tier compiles it.

### 20.5 Corpus and gate (pre-registered)

**Red first.** Before the IR lands, add trap-corpus entries for every
site in §20.3 that has no coverage today, plus accept entries for
§20.4. Entries whose two tiers already agree stay green and are
regression cover; any that diverge are the phase's work.

Exit criteria:

1. **A `TrapSite` variant that a lowering does not handle fails to
   compile.** Demonstrate it — the tracking entry records what the
   build error looks like when a variant is added and one arm is left
   out. This is the phase's reason to exist and the one criterion that
   cannot be waived.
2. The set of emitted checks is derived from HIR, and elision (§10a)
   is decided there. Both tiers emit checks for the same sites on the
   same program.
3. Every §20.3 site has trap-corpus coverage comparing
   `(kind, message, position, pre-fault stdout)` across tiers —
   **except** the three the Red stage showed cannot be compared:
   stale-coroutine (no C mode), the Q6 lifetime checks (§8.1b makes
   the C side unspecified by contract), and allocation failure (not
   reachable without fault injection). Each of those carries an entry
   recording *why* it is not compared, so an unverified site is visible
   rather than absent.
4. §20.4's two defects fixed, with corpus entries.
5. Standing gate green; `tsc` clean; no accept `.expected` moves
   except where §20.4's fixes make a previously-invalid program valid,
   which must be named in the tracking entry.
6. **Benchmarks re-run and recorded.** This phase moves the emission
   of division and indexing again — the paths P19 measured at 82
   instructions against 39. Emitted-C's ratio is reported against
   P19's 1.53×; a regression is a finding, not a cost to absorb.

### 20.6 What landed

**Green, 561 tests, 0 failures.** No pre-existing accept `.expected`
moved; the only additions are `a74` and `a75`, which §20.5 named as the
one permitted reason.

**The criterion that could not be waived is met, and was demonstrated
rather than asserted.** A throwaway `TrapSite::CompileProbe` variant
was added, handled in HIR and in the JIT, and deliberately left out of
the C emitter. The build failed:

```
error[E0004]: non-exhaustive patterns:
  `&subscript_compiler::hir::TrapSite::CompileProbe { .. }` not covered
  --> codegen/src/cemit.rs:927:15
```

Neither lowering's `match site` carries a catch-all arm, so this is
construction and not a test that has to be remembered.

`compiler/src/trap_sites.rs` derives the ordered site sequence in HIR,
elision included. The C tier now traps on both narrowing faults,
closing the type-safety hole the Red stage found. Template allocation
sequences match across tiers, and `a74`/`a75` — the two pre-existing
defects §20.4 pulled in — are fixed with JIT-derived goldens.

**Emitted-C measured 1.53×, unchanged from P19: no regression.** The
`perf-gate` command still exits non-zero because the Cranelift
ship-AOT and dev-JIT thresholds remain missed, which is the
pre-existing §11 situation that motivated C emission and not a P20
finding. Recorded because a non-zero exit that is *expected* should be
written down, or the next reader treats a real failure as routine.

## 21. P21 — the allocation path: fault injection and per-allocation attribution

Owner decision 2026-07-26. Two items that arrived separately and are
done together because **both touch `Context::alloc` and the 16-byte
allocation header**, and §20.4's rationale applies unchanged: splitting
them means touching that path twice.

§18.2e is superseded by §21.2.

### 21.1 Allocator fault injection

**The gap.** `TrapKind::AllocationFailure` has six raise points across
the two allocator modes and **no corpus program can reach any of
them**: there is no source-level memory quota, and exhausting real
memory is neither safe nor deterministic under overcommit. P20's `t26`
records the gap; the site is represented in the IR and unverified.

**Why this is not tidiness.** Three things are currently untested, and
the third has already happened once:

1. **The two tiers have different allocators** — the dev tier makes an
   individual system allocation per object tracked in a map; the ship
   tier (§8.1b) bump-allocates size-class blocks in chunks, with
   individual allocations above `LARGEST_BLOCK`. For the same program
   they fail at different moments, and a single object allocation can
   surface on the ship tier as a *chunk* allocation failing.
2. **`alloc` returns null on failure and the check happens after.**
   Generated code therefore holds a null payload for a window. That is
   the shape P19 found with `subscript_scratch` — a fault recorded, then
   execution continuing through a poisoned value, which there wrote 320
   bytes into a 256-byte buffer. Nothing looks at this one.
3. **Allocation *sequences* differed between the tiers until P20**,
   which removed a C-only empty-string allocation emitted per non-empty
   template — 9 fault points against 7 for `` `x${a}y${a}` ``. Only the
   differential gate holds them equal now.

**The knob.**

```c
void subscript_rt_ctx_fail_alloc_after(subscript_rt_context*, uint64_t n);
```

The Context refuses the **n-th subsequent allocation**. This is the
same shape as `subscript_rt_ctx_set_now` and `subscript_rt_ctx_seed_random`
(§18.1): a host knob that pins a nondeterministic input so tests and
replays reproduce, which §0.3 already establishes as the pattern.

**Count, not bytes — and the reason is measured.** §18.2d established
that `live_allocations` is tier-independent while `live_bytes` and
`reserved_bytes` are not, the ship tier rounding to size classes.
So "fail the n-th allocation" gives both tiers **the same failure
point**; "fail past n bytes" would give them different ones and could
not be compared at all.

**Expect the first run to find divergences, and treat that as the
point.** Injection by count *is* a test of allocation-sequence parity.
Item 3 above shows the sequences were unequal as recently as P20, and
nothing but the differential gate holds them equal now.

**What it does not cover**, stated so the limit is not mistaken for
coverage: real out-of-memory under overcommit stays untestable. This
measures **the language's response** to a refused allocation, not the
operating system's behaviour — which is the part that matters, since
the question is whether both tiers report the same tuple at the same
point and neither continues through the null.

**Check first, before building anything:** the "not representable"
raise point (`Layout::from_size_align` failing) may already be
reachable from source through a sufficiently large `FixedArray<T, N>`,
since the checker reads `N` from the annotation. If it is, that one
site can be tested today and the injection work shrinks. Report the
answer either way.

### 21.2 Per-allocation attribution (supersedes §18.2e)

**A live-allocation figure cannot name the script variable behind it,
and the obstacle is definitional, not cost**: an allocation is not
bound to a variable — values move between variables and into fields and
arrays — and a leaked allocation is usually reachable from no named
variable at all, which is why it leaked.

The **allocation site** is both available and better suited. A loop
allocating ten thousand times has one site and no useful name.

`alloc(size, class_id, pos_id)` **already receives** the site's
`pos_id` and discards it, using it only if the allocation fails, so no
new value has to be threaded anywhere.

**The header's fourth word was not free, and this section said it
was.** *(Corrected 2026-07-26 during implementation.)* On the ship tier
that word held each classed block's **exact requested payload size**,
and `Context.collect`'s mark phase read it to know how far to trace. Storing
`pos_id` takes it, so the mark phase now traces **the whole size-class
payload capacity** instead.

That is safe, and the safety is a property of the allocator rather than
an assumption: a fresh block comes from an `alloc_zeroed` chunk, and a
block reused from a free list is re-zeroed across its **full capacity**
(`write_bytes(payload, 0, block_size - HEADER_SIZE)`) before the header
is re-armed. The padding a conservative trace now reads is therefore
always zero.

The cost is real and bounded: `Context.collect` scans up to the size-class
rounding of each block rather than its exact request — at most a factor
of two, on an operation that never runs unbidden (invariant 2). The
alternative, widening the header to 24 bytes, costs every allocation 8
bytes to save an explicitly-invoked operation some scanning, which is
the worse trade. **Recorded rather than left silent, because a future
reader finding `Context.collect` tracing padding should find the reason here.**

Only the dev tier and the ship tier's large-allocation path add a
genuinely new store; for classed blocks the store replaces one that was
already there.

```c
typedef void (*subscript_rt_alloc_visitor)(void* userdata, uint32_t class_id,
                                     uint32_t pos_id, uint64_t payload_bytes);
uint64_t subscript_rt_ctx_visit_live_allocations(
    const subscript_rt_context*, subscript_rt_alloc_visitor, void* userdata);
```

Read-only, like §18.2d's three figures, and with the same cost
character: O(live blocks), a diagnostic rather than a per-frame
counter.

**Two things this phase must settle, not assume:**

- **The `class_id` and position tables belong to the compiler, not the
  Context.** §18.1 records the same problem for a trap's `pos_id`: a
  host cannot turn either id into a name without an artifact this
  repository does not emit. Emit it, on the same principle as §18.1b's
  generated header — **generated, never hand-written**, with a
  byte-identical regeneration test. A hand-kept table drifts from the
  ids it claims to describe.
- **The extra store lands in the ship tier's arena path** (§8.1b),
  which is performance-sensitive; that tier's justification is that
  emitted C is close to hand-written C. One `u32` store per allocation
  is expected to be negligible — **measure it, do not assert it.**

### 21.3 Corpus and gate (pre-registered)

**Fault injection.** Trap-corpus entries covering an allocation failure
in each distinguishable position — a reference-class `new`, an array
literal, a `push` that grows, a string concatenation, a template, a
generator frame, and the checker-generated JSON `RawNew`. Each compares
`(kind, message, position, pre-fault stdout)` across tiers, which is
what P20 made possible. `t26`'s policy-only record is replaced by real
entries for every site injection can reach; any that remain
unreachable keep a policy record saying so.

**The null-continuation question is a gate item, not an aside.** For
each entry, the generated code must not read or write through the null
payload between the failed allocation and the check. Demonstrate it —
AddressSanitizer on the ship tier, as P19's review did for the
320-byte case.

**Attribution.** An accept entry is not the right shape, since the
output is host-side. A both-tier host test allocates a known
population from known sites, visits, and asserts the `(class_id,
pos_id, bytes)` triples. `live_allocations` agreeing across tiers is
§18.2d's contract and this test inherits it.

Exit criteria:

1. Every allocation-failure site injection can reach is covered, on
   both tiers, with identical tuples — or is recorded as unreachable
   with the reason.
2. No entry continues through the null payload; ASan clean.
3. The visitor reports the allocating site, and the generated id tables
   let a host resolve `class_id` and `pos_id` to names.
4. The regeneration test for those tables is byte-identical, as
   §18.1b's header is.
5. Standing gate green; `tsc` clean; no accept `.expected` moves.
6. **Benchmarks re-run.** The `u32` store is on the ship tier's
   allocation path. Report emitted-C against P20's 1.53×; a regression
   is a finding, not a cost to absorb.

## 22. P24 — two monotonic costs under invariant 2

Both items were found by measurement in earlier phases, recorded as
carried forward, and scheduled together on 2026-07-27 (owner) because
they are the same defect wearing different clothes: **something inside
the Context grows without bound in a way the program cannot control,
and no gate can see it.** One is binary size charged to every shipped
program; the other is `Context.collect()` time charged to every long-running
host.

Neither is a bug in what the code computes. Both are bugs in what it
costs, which is why the corpus never noticed.

### 22.1 The code-point table (`stdlib.md` §15.1, P23 carried forward)

`context::CODE_POINT_UTF8` is `[u32; 0x110000]` — **4,456,448 B**, the
UTF-8 bytes of every Unicode scalar at a stable address. It is the
largest single item in a shipped binary, **7× the regex engine**, and
every program that touches a string links it.

**It exists to supply an address, not a computation.** Encoding a
scalar to UTF-8 is a handful of instructions; `str_bytes` returns
`&[u8]`, a borrow, so the bytes must live somewhere addressable, and
the handle `subscript_rt_str_iter_code_point` hands out is a tagged integer
rather than a pointer into real memory.

**Its only consumer is `subscript_rt_str_iter_code_point`** — `for…of` over a
string, and `stdlib.md` §14.3's guarantee that the loop allocates
nothing. `charAt` calls `alloc_str` and always has. *(Both §15.1 and
the runtime's doc comment said `charAt`; corrected 2026-07-27.)*

**The astral range is the entire cost:**

| | scalars | bytes |
|---|---:|---:|
| BMP (`< 0x10000`) | 65,536 | 262,144 |
| **astral (`≥ 0x10000`)** | **1,048,576** | **4,194,304** |

#### What must hold

1. **BMP loses nothing.** Every scalar below `0x10000` — all Latin,
   Greek, Cyrillic, Arabic, Hebrew, Devanagari, kana, common CJK,
   Hangul — keeps its static-table address and allocates nothing. That
   is essentially all text.
2. **Astral is bounded by distinct scalars used, not by iterations.**
   A per-Context intern map: the first occurrence of an astral scalar
   allocates its bytes, every later occurrence returns the same handle.
   A ten-thousand-iteration loop over `"😀"` repeated allocates
   **once**.

   This is the bound `stdlib.md` §15.5a already gives the
   compiled-pattern cache, and it is chosen for the same reason: a
   per-iteration allocation under invariant 2 accumulates until
   `Context.collect()`, which is the defect §15.5a was written for.
3. **The intern map is Context-owned and is not swept.** No program
   reference reaches it, so a sweep would free bytes a live handle
   still points at. It lives and dies with the Context, like the
   compiled-pattern cache.
4. **`str_bytes` stays total.** A handle for an astral scalar that was
   interned in one Context must not be read against another.

**The honest bound, stated rather than glossed:** a program iterating a
string containing a million *distinct* astral scalars allocates a
million times. That is not a shape any real text has, and it is the
price of not carrying 4 MB in every binary.

### 22.2 The dev-tier allocation map (`p4-performance.md`, P21 carried forward)

The dev tier realizes `Context.free`/`Context.collect` by **retain-and-poison**
(§8.1a): freed bytes stay owned by the Context with a dead header, so a
stale handle traps instead of reading reused memory. §8.1a accepted the
*memory* cost of that retention as the price of the guarantee.

**What was not anticipated is that the retention also costs sweep
time.** Dead entries stay in the same map the sweep walks, so **sweep
is proportional to every allocation ever made**, not to what is live.
Measured inside one run as entries grow 120,005 → 720,005: sweep
**0.73 → 3.48 ms, linear**, while mark stays 14–16 ms because mark *is*
proportional to live data.

A host calling `Context.collect()` per level transition or inside a frame
budget therefore sees its collect cost **rise monotonically for the
lifetime of the Context, regardless of how much is live.** `a16`, `a51`
and `a70` allocate too little to show it; a real embedding would not.

#### The fix must not weaken the guarantee

The obvious fix — dropping dead entries — trades the diagnostic away,
and the diagnostic is what §8.1a bought the retention for. It is not
acceptable here.

**The defect is that dead entries sit in the swept structure, not that
they exist.** Segregate them: a sweep that walks only live entries is
proportional to the live set, and every dead entry stays exactly as
poisoned and as trappable as it is today. Memory retention is
unchanged — that cost §8.1a already accepted and this phase does not
reopen.

If segregation proves impossible without changing what traps, **stop
and report it** rather than bounding the poison window; a smaller,
correct win is preferred to a larger one that quietly narrows a
guarantee.

~~**Fold in the adjacent measured finding:** reserving the
`allocations` map avoids two rehashes, worth **~15 ms of 229 ms** on
the same workload.~~

**Withdrawn 2026-07-27, on measurement: the segregation above removed
the rehashes this clause was written to avoid.** Pre-P24 the map's
capacity grew `229,376 → 458,752 → 917,504` because dead entries stayed
in it; Part B leaves the live set only, and one bounded rebuild from
tombstone pressure.

Re-measured on the same `collect` workload:

| `reserve` | median |
|---:|---:|
| 0 (as built) | 208.092 ms |
| 120,005 | 204.300 ms |
| 240,000 | 201.089 ms |
| **720,005** — the pre-P24 peak this clause implied | **215.524 ms** |

**Reserving the figure the original finding pointed at is now the worst
of the four.** A 240,000 reserve does buy ~7 ms, but it pre-pays a
workload-shaped map on **every** dev Context, and Context construction
sits outside the measured span — so that number is a different trade
from the one this clause described, and is not adopted by default.

A clause whose premise a later change removes is withdrawn with the
measurement, not quietly dropped.

*(A third finding from that run — setting any explicit QoS class makes
`particles` 9.5% faster, 622 → 563 ms — is benchmark-harness hygiene,
not runtime behaviour. It belongs to `benchmarks.md`, not here.)*

### 22.3 Corpus and gate (pre-registered)

**Accept.** A `for…of` over a string of BMP scalars asserting the
existing output unchanged; one over a string of repeated astral scalars
(the interning case); one mixing BMP and astral; one over a string of
several *distinct* astral scalars. Each pinned on both tiers.

**Attribution, not a corpus entry**, for the allocation counts, since
the observable is host-side: a both-tier test asserting that iterating
`n` repetitions of one astral scalar performs **one** allocation, and
that BMP iteration performs **none** — using
`subscript_rt_ctx_visit_live_allocations` (§21.2), which already reports
`(class_id, pos_id, payload_bytes)`.

**Traps.** None new. A malformed handle is `str_bytes`'s existing
contract.

**Gate.** The standing differential gate byte-exact on both tiers;
`tsc` clean; no pre-existing accept `.expected` moves — a golden that
moves here means iteration output changed, which nothing in this phase
should do.

### 22.4 Exit criteria (kill or pass, pre-registered)

1. **Binary size drops by 4,194,304 B ± 64 KB**, measured by the
   `regex-size-gate` matched pair (`stdlib.md` §15.7). Both reference
   constants in that gate move; **that is the expected result, not
   drift**, and the commit must say so. A drop materially smaller than
   4 MB means the astral table is still reachable from somewhere and is
   a finding.
2. **BMP `for…of` allocates nothing**, asserted by the attribution
   test, and its emitted C is unchanged on the ship tier.
3. **Astral `for…of` allocates once per distinct scalar**, asserted by
   the same test across at least 1000 iterations.
4. **Sweep time is proportional to live entries, not to cumulative
   allocations.** Re-run the measurement that found it: as entries grow
   120,005 → 720,005 with the live set held constant, sweep must stay
   **flat within noise** instead of going 0.73 → 3.48 ms. A sweep that
   still grows linearly fails this phase.
5. **Every dev-tier use-after-delete that traps today still traps**,
   with the same kind, message and position, at every distance from the
   free — including one deleted before 700,000 subsequent allocations.
   This is the guarantee §8.1a bought and the one this phase is most
   likely to break.
6. **Benchmarks re-run.** Report emitted-C against P21's 1.52× and the
   `collect` workload against its recorded figure. A regression is a
   finding, not a cost to absorb.
7. Standing gate green; `tsc` clean; clippy at its baseline.

### 22.5 What landed

Both parts landed. Every §22.4 criterion was measured and met;
`specs/tracking/p24-monotonic-costs.md` carries the numbers. **One
clause of §22.2 did not land and is withdrawn there with its
re-measurement** — the map `reserve` fold-in, whose premise Part B
itself removed.

**The astral range is gone and the guarantee it bought is narrower than
§14.3 said.** `stdlib.md` §14.3 was headed "the loop allocates nothing"
and §22.1 quoted it as the property the table existed to serve. The
**iterator** costs nothing, which is what that section is about and
which is unchanged; the **element** now allocates once per distinct
astral scalar. `stdlib.md` §14.3a states the bound and §14.3's heading
no longer overclaims.

#### The ship-tier `tree` movement was `Context`'s size, not this phase

The `tree` benchmark — 30×131071 alloc/delete pairs — moved on both
tiers. Only one of the two movements belongs to P24.

**dev-JIT, ~673 → ~500 ms: the phase's own mechanism.** §22.2 moved
dead entries out of the live map, so every later allocation's hash
insert works against the live set rather than against every allocation
ever made. This is the cache-hostile growth §8.1a identified for the
ship tier and fixed there by releasing; P24 fixes it for the dev tier
without giving up retain-and-poison. Confirmed by A/B: padding the
pre-P24 struct does **not** reproduce it.

**ship, ~110 → ~93 ms: layout, and not this phase.** The Phase Review
established the attribution by measurement, against a first record that
named the wrong cause:

- Restoring `CODE_POINT_UTF8` to `[u32; 0x110000]` at P24's HEAD leaves
  ship `tree` unmoved — **92.828 ms** against 92.484 ms. So it is *not*
  the 4.19 MB of static data.
- `size_of::<Context>()` went **920 → 1024 B**; the three new fields
  add exactly 104 B and `Context` is `#[repr(C)]`, so every later field
  shifts. Inserting **104 bytes of dead padding** into the otherwise
  untouched pre-P24 struct reproduces the entire ship win — 120.5 →
  91.8 ms. 64 bytes suffices; 8 bytes does not.

**A win a phase did not cause must not be recorded as one**, and a
figure this sensitive must not be read as a runtime property: the
published ship `tree` row moves ±24% on a struct-size change with no
semantic content. `benchmarks.md` carries that caveat.

*(The first record of this, in the benchmark commit, said "the only
ship-visible change is 4.19 MB less static data, i.e. binary layout".
The instinct — refuse the credit — was right; the named cause was
measurably wrong.)*

## 23. P25 — no header is privileged

Design invariant 4 says the host presents C headers and the language
binds those, and that **no specific host header is privileged by the
language**. The binding path does not implement that today: it binds
exactly one header, `corpus/interop/interop.h`, and a second header
cannot reach either tier.

Found 2026-07-28 while contracting `examples.md`, whose C-integration
examples bind a host facade of their own. The corpus never noticed
because the corpus has only ever had one header to bind.

### 23.1 What is wrong, with evidence

1. **The ship tier includes the fixture by name.** `codegen/src/cemit.rs`
   emits `#include "interop.h"` whenever the module has any foreign
   function, and declares the callback trampoline with that header's
   `SubStringView`.
2. **The ship tier names the fixture's descriptor structs.**
   `interop_array_pair_desc` maps an element type to a fixed C aggregate
   name — `SubBufferView` for `u32`, `SubSlice<T>` otherwise. Those
   names belong to the fixture, not to the language.
3. **The mirror throws away the name the emission needs.** `bindgen`
   absorbs a `(pointer, count)` descriptor into `T[]` at use sites and
   emits no record of the C struct it came from, so the emission has
   nothing to recover and a table in the compiler is the only thing
   left.
4. **The dev tier resolves foreign symbols from a fixed list.**
   `codegen/src/jit.rs` holds an `extern "C"` block naming the fixture's
   28 symbols and registers them by address. `cranelift-jit` 0.125.4
   falls back to `dlsym(RTLD_DEFAULT)` on Unix and to `GetProcAddress`
   over loaded modules on Windows, so an unregistered symbol resolves by
   accident on Unix and is not guaranteed to resolve on Windows —
   a portability trap, not a substitute for registration.
5. **The link line is the fixture's.** `codegen/src/aot.rs` passes
   `-I corpus/interop` and `corpus/interop/interop.c` unconditionally,
   and `codegen/build.rs` compiles that file into the crate.

The consequence is one sentence: **a host cannot bind its own header.**
That is the project's headline interop claim, and it is currently true
only of the fixture.

### 23.2 The rule

> Every C name the emitted code needs is **recovered from the bound
> mirror**. The compiler, the two tiers, and the runtime contain no
> identifier from any particular header.

The fixture keeps working by becoming **an argument** — one native
library among any number the caller supplies — rather than a case in the
binding path. The test of the rule is deletion: removing the fixture
must require touching test scaffolding only, never `compiler/src` or
`codegen/src`.

### 23.3 Mirror provenance

`bindgen` records in the mirror what the ship tier's C emission needs and
cannot otherwise know:

- the **include spelling** for the header the mirror was generated from,
  as the host would write it (`engine.h`), never a filesystem path;
- for every absorbed `(pointer, count)` descriptor, the **C aggregate
  name**, its element C type, its mutability (the const borrow and the
  out/mutable array are different types — §14.3), and the parameter it
  belongs to;
- the **C typedef name** of every callback type, so the trampoline can be
  cast to the type the header declares at the point of use.

Constraints on the spelling, not the spelling itself: it must keep the
mirror `tsc`-clean (invariant 5), it must be generated and covered by the
byte-identical regeneration test (§12.2), and a mirror the compiler
cannot parse provenance from is a **loud error at ingestion**, never a
silent fallback to a built-in table. Recording it per parameter rather
than per element type is required: one header may declare both a const
borrow and a mutable out-array of the same element type, and the
element type alone does not distinguish them.

### 23.3a One callback shape is bindable, and a mirror must refuse the rest

The runtime has **exactly one** C-ABI callback trampoline, and its
signature is fixed:

- `runtime/src/ffi.rs` — `subscript_rt_cb_trampoline(message: SubStrView,
  userdata1: *mut u8, userdata2: *mut u8)`, three C parameters;
- `codegen/src/lower/mod.rs` declares it to the dev tier as
  `[I64, I64, I64, I64]` — the view flattened to two registers plus the
  two userdata;
- `codegen/src/cemit.rs` stores it into a header's callback field through
  a cast to that field's declared typedef.

So the **only** bindable callback is `(string view, userdata, userdata)
→ void`, and the language function value behind it receives
`(message, userdata1, userdata2)`.

**Nothing in the toolchain checks this.** A mirror may declare a callback
of any shape, the checker accepts a matching lambda, and the lowering
installs the three-parameter trampoline behind it. The C API then calls
it with its own signature: an extra leading parameter lands where the
view's data pointer is read, and the view's length lands where the
binding pointer is read and dereferenced.

*(Found 2026-07-28. The examples facade's first draft declared
`(EngineEventKind, EngineStringView, void*, void*)`; `bindgen` emitted the
mirror, and no stage of either tier objected.)*

**`bindgen` must reject a callback typedef whose signature the trampoline
cannot serve**, naming the typedef and the supported shape — the §23.8
kill criterion applied to callbacks: a mirror the tiers would mis-marshal
must not be written. A header that needs to deliver more than the view
and the two userdata slots delivers it through a separate call the script
makes, not through an extra callback parameter.

**The check is on reachability, not on presence.** It applies to a
function-pointer typedef that can cross the boundary — used as a mirrored
struct's field, or as a foreign function's parameter or return. A typedef
a host declares and the boundary never touches cannot be mis-marshaled,
and rejecting the header for it would make headers unbindable for a
reason the language does not have. Such a typedef is simply not mirrored.

Rejecting on presence was the first implementation, and it fails P25's own
purpose: one unrelated function pointer anywhere in a production header —
an allocator hook, a debug sink — would refuse the whole header.

Generalizing the trampoline to arbitrary callback signatures is **not in
P25's scope**. This section makes the existing limit visible and loud; a
host that needs more is a later phase with its own contract.

### 23.3b Carried forward — the inherited-precedent audit

`specs/tracking/inherited-precedent-audit.md` pre-registers a sweep for
one defect class this phase produced: a requirement carried from an older
artifact by analogy, without re-deriving whether the destination needs it.
One §23.3 provenance record is already named there as suspect. The sweep
runs at or after this phase's Phase Review; its scope and pre-registered
outcome are in that file, not restated here.

### 23.4 Ship tier

The emitted C includes **each ingested mirror's header**, in ingestion
order, and no other. Descriptor struct names and callback typedef names
come from provenance.

The generic callback trampoline is declared with a **locally emitted,
layout-identical view struct** under a reserved `sub_` name rather than
with a header's type; the runtime already defines that layout as
`SubStrView` in `runtime/src/ffi.rs`. At the point where the trampoline
is stored into a header's callback field it is cast to that field's
declared C typedef, recovered per §23.3. Layout identity (invariant 1)
is what makes the cast sound; it is not a convenience.

### 23.5 Dev tier

Foreign symbol resolution becomes explicit and caller-supplied. No
`extern "C"` block naming a particular header's symbols remains in
`codegen/src`. Registration is by address, as today; the fixture's table
moves to the corpus gate's own support code.

**The dlsym fallback is not relied on.** A foreign symbol a caller did
not register is a run error naming the symbol, not a lookup that happens
to succeed on one platform. This is the Windows half of the portability
record (`specs/tracking/windows-portability.md`).

### 23.6 The surface a caller uses

One value type describes a native library the caller links: its include
directories, its C sources for the ship tier's compile, and its symbol
table for the dev tier's JIT. Both runners take a set of them. Taking
function addresses across a language boundary is `unsafe`; the
constructor carries the SAFETY contract (the addresses outlive every run
and match the C signatures the mirror declares).

`emit-c` (`codegen/src/bin/emit-c.rs`) additionally accepts explicit
source and mirror paths instead of only a corpus entry id, and a flag to
suppress the generated `entry.c` — a host that owns `main` supplies its
own. The corpus-entry form stays, because `device-link.sh` uses it.

### 23.7 Corpus and gate (pre-registered)

**A second fixture, and the two bound together.** The examples' host
facade (`examples/engine/engine.h`, `examples.md` §4) is the second
header. Two new gate programs:

1. one binding **only** the engine facade — the first time any header
   other than the fixture is bound on either tier;
2. one binding **both** headers in a single program — the proof that
   binding is per-mirror and that no include, descriptor name, or symbol
   table is global.

Both run under dev-JIT and ship-C-AOT and are compared byte-exact, as
every corpus entry is. They live in the examples gate crate
(`examples.md` §7.6) with committed goldens, not in `corpus/accept/`:
that crate is what links `engine.c`, and the corpus stays free of a
dependency on `examples/`.

**No existing golden moves.** This phase changes which C names the
emission writes down, not what any program computes. An `.expected` that
moves is a finding.

#### 23.7a Corpus first — an extension payload read back through the chain

The intrusive extension chain is one of the five §4 patterns, and the
project exercises only half of it: `subDeviceCreate` **counts** nodes
(`interop.c`), no accept entry mentions `SubChainExtA`–`SubChainExtD`
(`grep ChainExt corpus/accept/` is empty), and no fixture function reads
an extension's payload. Reading the payload is the point of the pattern —
it is why a chain node carries a tag — and nothing pins it.

*(Found 2026-07-28 by the fresh-context review of the examples facade,
whose option walker reads payloads. Recorded here rather than in the
examples work: a semantic no corpus entry covers is a corpus gap, and
`examples.md` §1 forbids an example from being the first place it appears.)*

Two consequences, both before the facade's walker is accepted:

1. **The fixture grows one payload-reading walk** — a function that walks
   a chain, switches on `sType`, and folds each extension's payload
   scalars into an observable. Structs only, as §12.1 requires; the
   `offsetof` suite already mirrors `SubChainExtA`/`SubChainExtB`.
2. **An accept entry pins it**: a program that builds an extension in
   script, passes that extension's **embedded header field** into the
   `Struct | null` chain slot, and observes the payload the callee read
   back. This is the semantic the facade depends on — that the address
   crossing the boundary is the live struct's own storage, not a copy of
   the header field — and it is the one a node count cannot discriminate,
   because a copy of a header has the same `next` and yields the same
   depth.

**The precondition the pattern carries is documented, not engineered
away.** A callee that switches on a tag and casts to the matching
extension assumes the node is that extension's embedded header; a
production chain API assumes exactly this. A facade may therefore keep
payload-bearing options, provided the header states the precondition at
the declaration — and provided the entry above exists, so the spelling the
facade needs is the spelling the corpus teaches.

### 23.8 Exit criteria (kill or pass, pre-registered)

1. **A header other than the fixture binds and runs**, byte-identical on
   both tiers, against a committed golden (§23.7 program 1).
2. **Two headers bind in one program** (§23.7 program 2), byte-identical
   on both tiers.
3. **No fixture identifier or path survives in the binding path.** A
   search over `compiler/src` and `codegen/src` returns nothing for the
   fixture's type and symbol names (`SubSlice`, `SubBufferView`,
   `SubStringView`, `SubLogCallback`, `SubWaitList`, the `sub…` foreign
   symbols) **and** for the fixture itself — `interop.h`, `interop.c`,
   the `corpus/interop` path, and any identifier naming it such as
   `interop_dir` or `register_interop`. Matches under `corpus/`,
   `examples/`, `tests/` and `build.rs` are expected and are not
   violations.

   *(The name list was type names only until 2026-07-28. Stage 3 satisfied
   it by rewriting a doc comment while `aot.rs` still resolved
   `corpus/interop` and compiled `interop.c` into every link — Stage 4's
   scope, so not a violation there, but a criterion satisfiable by
   renaming is not a criterion. Criterion 4 is what actually establishes
   the property; this one is its cheap precheck and must not be weaker.)*
4. **Deleting the fixture touches test scaffolding only** — demonstrated,
   not asserted: the deletion compiles with no edit under
   `compiler/src` or `codegen/src`. The deletion is not committed.
5. **A missing registration fails loudly.** A program calling a foreign
   function whose symbol was not supplied reports an error naming the
   symbol, on both tiers and on both a Unix and a Windows host — not a
   dlsym hit.
6. **A mirror without parseable provenance is rejected at ingestion**,
   with the offending mirror named.
7. **`bindgen` regeneration stays byte-identical** (§12.2) with the
   provenance records present, and the mirror stays `tsc`-clean.
8. **A callback shape the trampoline cannot serve is rejected** (§23.3a),
   naming the typedef and the supported shape, with a test per rejected
   shape — an extra parameter, a missing userdata slot, a non-`void`
   return — and a test that an **unreachable** function-pointer typedef of
   an unsupported shape does *not* refuse the header.
9. Standing differential gate green; `tsc` clean; clippy at its baseline.

**Kill criterion.** If a descriptor's C name cannot be recovered for some
header shape the generator otherwise accepts, `bindgen` **fails on that
header** naming the construct, as it already does for an unmapped type
(§13.1). A mirror the ship tier would mis-marshal must not be written.

## 24. Q32 — string-literal union aliases

Owner decision 2026-07-31 (collisions.md Q32, C7 exception; requested
by the downstream WebGPU binding project, HANDOFF/REPORT exchange).
The decision text lives in Q32; this section is the implementation
contract.

### 24.1 Checker

`type Name = "a" | "b";` at module level declares a closed literal
set, nominal by alias. The checker: accepts member literals in
alias-typed contexts and rejects non-members; treats two
same-membered aliases as distinct; accepts `===`/`!==` between
same-alias values and against member literals, rejecting comparisons
with `string` or other aliases; rejects Q32 aliases in boundary
(mirror) signatures. Inline literal unions keep today's S011
rejection. The rejection codes are whatever the checker's existing
mismatch paths produce — pinned by the corpus entries, not minted
anew unless no existing code fits.

### 24.2 Lowering (both tiers)

An alias value is an `i32` discriminant, member index in declaration
order. Each alias carries a static string table used only when a
value is formatted (template literals); `===`/`!==` lower to integer
compares. The two tiers agree byte-exactly on the printed member
strings (the standing gate).

### 24.3 Corpus

`a91-string-literal-union` (accept): two aliases, one pair sharing
member spellings; assignment, parameter passing, equality against a
literal and a same-alias value, and template-literal printing —
golden output contains the member strings. `r87`: non-member literal
in an alias context, (code, line) pinned. `r88`: inline literal
union in a parameter annotation, S011, pinned. `r89`: assignment
across same-membered aliases, `tsc`-clean, pinned.

### 24.4 Exit criteria (pre-registered)

1. `a91` runs byte-identical under both tiers, member strings in the
   golden.
2. `r87`/`r88`/`r89` pin (code, line); `r89` type-checks under stock
   `tsc` (the strictly-narrower proof).
3. A `cemit` unit test asserts an alias equality lowers to an
   integer compare — no string-comparison call at the comparison
   site in the emitted C.
4. No existing golden moves; full gate and `tsc` gate green; the
   zero-warning sweep is unaffected.
5. Checker unit tests: member accepted, non-member rejected,
   cross-alias rejected, boundary-signature rejection.

## 25. Q33 — literal-constructible descriptor classes

Owner decision 2026-07-31 (collisions.md Q33, C1/C7 exceptions;
downstream request R1). The decision text lives in Q33; this section
is the implementation contract.

### 25.1 Checker

`@Descriptor class` declares a data-only reference class: any
constructor, method, or `extends` clause is rejected; each member is
required (`name!: T`) or defaulted (`name?: T = expr`); `?` without
an initializer, or an initializer on a `!` member, is rejected. An
object literal type-checks only in a context expecting a
`@Descriptor` class, against exactly that class: missing required
members, excess members, and literals against unmarked classes are
rejected; nested literals and array-of-literal members check
recursively; Q32 alias members and defaults compose. `?` outside a
`@Descriptor` class keeps today's rejection. Codes: reuse the
existing paths (mismatch, closed-property) where they fit; the
corpus entries pin whatever fires.

### 25.2 Lowering (both tiers)

A constructing literal lowers to the class's ordinary allocation
followed by member stores — explicit members from the literal,
omitted members from their default expressions, evaluated at the
construction site. Defaults are per-construction (a fresh nested
descriptor default is a fresh allocation, not a shared instance).
Byte-identical behavior across tiers under the standing gate; no
runtime additions.

### 25.3 Corpus

`a92-descriptor-literals` (accept): a descriptor with required,
defaulted, Q32-alias, nested-descriptor, and array-of-descriptor
members; constructions covering full literals, omission taking
defaults, `{}` against an all-defaulted descriptor, nesting, and an
argument-position call — golden prints prove filled defaults and
overrides under both tiers. Reject: `r90` missing required member,
`r91` excess member, `r92` literal against an unmarked class
(`tsc`-clean — stock TS accepts it structurally; the
strictly-narrower proof), `r93` `?` member without initializer in a
descriptor, `r94` a method in a descriptor class, `r95` `new` on a
descriptor class (`tsc`-clean; added at landing — literal
construction is the only construction).

### 25.3a Amendment — literals through `Descriptor | null` (2026-08-02, R17)

Downstream bug report, reproduced at the pin: contextual typing for
descriptor object literals stopped at `Type::Class` and never
unwrapped `Nullable`, so a literal whose contextual type is
`DescriptorClass | null` — a defaulted member
(`m?: D | null = null`), a required member (`m!: D | null`), a
parameter, or the same positions under nesting and array
elements — rejected with a generic S100 while `null` and a typed
temporary were accepted. The shape is real and generated: §33's
`[nullable]` struct-pointer members mirror as exactly these
member types.

Rule: **in a contextual position typed `D | null` where `D` is a
descriptor class, an object literal takes the descriptor arm and
`null` keeps its meaning** — at any depth, matching the
non-nullable behavior of §25 otherwise (defaults, required-member
enforcement, excess rejection). A literal against
`PlainClass | null` keeps S005 (nominal rejection), pinned.

Corpus: `a117-descriptor-literal-nullable-member` (accept,
Red-first — the entry reproduces the rejection at the pre-fix pin
and lands with the fix): both member kinds, `{ m: {} }`,
`{ m: null }`, omission taking the `null` default, and the
array-element nesting from the downstream's controls; observation
`tsc`-strict-compatible (presence checks in template position for
the `?`-declared member, full narrowed reads on the
required-nullable member). `r116-object-literal-nullable-class`
(reject): `{}` against `PlainClass | null` pins S005 in the
nullable position (`tsc` status probed and recorded).

### 25.4 Exit criteria (pre-registered)

1. `a92` runs byte-identical under both tiers; the golden shows a
   default-filled value, an overridden value, and a nested default.
2. `r90`–`r94` pin (code, line); `r92` type-checks under stock `tsc`
   (verified standalone, recorded in its header).
3. The prelude declares `Descriptor`; the `tsc` gate is green with
   `a92` in the include set.
4. No existing golden moves; full gate green; the zero-warning sweep
   is unaffected.
5. Checker unit tests: required-present, default-filled,
   excess-rejected, unmarked-class-rejected, and the two member-form
   rejections.

## 26. Q34 — async/await, poll-driven

Owner decision 2026-07-31 (collisions.md Q34, C8 revision; downstream
request R4). The decision text lives in Q34; this section is the
implementation contract. The `tsc`-acceptance of the whole surface —
ambient `Context.suspend(): Promise<void>`, `async function` chains,
`await` unwrapping — was probed against stock `tsc` before
contracting.

### 26.1 Checker

`async function` declarations (module-level and exported) with
explicit `Promise<T>` return annotations; `await` legal only inside
them, applied to exactly the two Q34 awaitable forms. Rejected, with
corpus pins: `new Promise` (r96), any `.then`/`.catch`/`.finally`
call (r97), `Promise` statics (r98), `await` outside async (r99), a
floating async call statement (r100 — must be awaited). Async calls
are direct calls in await position; async function values are not
first-class (no storing/passing references to them as promises).
S013's meaning narrows accordingly: it now rejects the Promise
object surface, not the keywords; `r14-async` retires (file and
harness row removed — Q34 makes its pinned construct legal).

### 26.2 Lowering (both tiers)

An async function lowers onto the existing Context-owned coroutine
frame machinery: `await Context.suspend()` is a suspend point;
`await f(...)` allocates the callee frame, runs it to its first
suspension or completion, and on suspension suspends the whole
chain up to the root. A root invocation (from the host symbol) runs
to first suspension and registers a pending root; each
`async_step` resumes each pending root once, in kick order,
resuming at the innermost suspended frame; a root that completes
leaves the pending set. Frames free with their Context (teardown
drops, no continuations). Byte-exact across tiers under the
standing gate.

### 26.3 Runtime C API and drivers

```c
uint64_t subscript_rt_ctx_async_pending(const subscript_rt_context*);
uint64_t subscript_rt_ctx_async_step(subscript_rt_context*);  /* returns remaining */
```

`async_step` on a trapped Context is a no-op returning the pending
count (the trampoline precedent); on an empty pending set it returns
0. **Standard-runner convention** *(added 2026-07-31 when the first
contract draft left a94 undrivable — the gate invokes only `main`)*:
the generated AOT entry, the JIT runner, and `subscript run` invoke
`main`, then every **other** exported async function in declaration
order, then pump `async_step` to quiescence. A host embedding the
runtime kicks whatever it chooses — the convention is the runners',
not the language's. Synchronous programs see no change (no async
exports, empty pump), so no existing golden moves. The generated
host header documents both functions; hot-reload's §8.2 staleness
applies to suspended async frames unchanged.

### 26.4 Corpus

`a93-async-chain` (accept): leaf async polling a counter via
`Context.suspend()`, middle async awaiting the leaf, async `main`
awaiting the middle; prints pin the resume ordering and final
values. `a94-async-two-roots`: two async exports kicked then pumped
by the harness, interleaving deterministically in kick order.
`a95-interop-async-await`: an async function awaiting a foreign
poll (the interop fixture gains a deterministic poll —
`subDevicePoll`-style, completing after a fixed call count — only if
no existing function fits); absorbs the Q1 request. Rejects
`r96`–`r100` as listed in Q34, `r100` `tsc`-clean (verified
standalone, recorded in its header). `r14-async` deleted.

### 26.5 Exit criteria (pre-registered)

1. `a93`–`a95` byte-identical under both tiers, driven by the same
   pump-to-quiescence entries the gate already uses.
2. `r96`–`r100` pin (code, line); `r100` type-checks under stock
   `tsc`; `r14` removed from tree and harness.
3. `async_step` determinism: a unit test pins the kick-order
   interleave; a trapped-Context step is a no-op (unit test).
4. Prelude declares `Context.suspend`; `tsc` gate green with the new
   accept entries included.
5. No existing golden moves (sync programs are unaffected by the
   pump); full gate green; zero-warning sweep unaffected.
6. Hot reload: a suspended async frame resumes stale per §8.2's
   existing trap (unit test at the reload layer).

## 27. R5 — scalar array-pairs at parameter position

Owner decision 2026-08-01 (downstream request R5, blocking its
buffers+queue area). Diagnosis first, because the request's own
guess was wrong in a useful way: the bindgen scalar table
(`lang_scalar`) already maps the full `stdint.h` set — `uint8_t` was
never unmapped as a scalar. The failing site is **pointer-to-scalar
at a boundary position**, which routes to the named-type registry
and correctly fails loud. What is missing is a rule for the shape
the downstream facade needs: a count-first scalar pair as two
adjacent **function parameters**.

### 27.1 The rule

The struct-level `<name>Count` / `<name>` adjacency rule (§13.2)
extends to parameter lists. Two adjacent parameters
`size_t <name>Count, [const] S* <name>` — where `S` is any
`lang_scalar` scalar — mirror as one parameter `<name>: S[]`:

- **`const S*` (script → C).** The existing input-direction pair
  semantics: the language array's `(ptr, len)` cross zero-copy; the
  callee reads `<name>Count` elements.
- **`S*` (C fills, R5.2).** The count crossed is the script array's
  **length**; the callee writes up to that count in place and never
  grows the array; the writes are visible to the script after the
  call, byte-identically in both tiers. This is §14.3's out-array
  direction applied to scalar elements at parameter position.

Non-adjacent count/pointer parameters, non-scalar pointer
parameters, and pointer returns keep today's fail-loud rejection —
the rule is the adjacency spelling, not a general pointer surface.
Struct-level scalar pairs (registered aggregates) are unchanged.

### 27.2 Corpus and fixture

The interop fixture gains one function per direction — a
deterministic byte consumer (`const uint8_t*`: e.g. summing) and a
deterministic filler (`uint8_t*`: a fixed pattern), plus a `u16`
variant of one direction (the downstream index-data case) — and the
mirror regenerates through `subscript bind`. `a96` (accept): script
builds a `u8[]`, passes it to the consumer, prints the result;
passes an array to the filler and prints the elements after the
call (the visibility pin); exercises the `u16` variant. Bindgen unit
tests pin the parameter-pair mapping for both directions and that a
non-adjacent pair still fails loud.

### 27.3 Exit criteria (pre-registered)

1. `a96` byte-identical under both tiers, including the
   after-the-call element prints for the filled array.
2. Bindgen tests: `(size_t xCount, const uint8_t* x)` and
   `(size_t xCount, uint8_t* x)` parameters mirror as `x: u8[]`; a
   `u16` variant likewise; a lone scalar-pointer parameter and a
   non-adjacent pair keep the fail-loud error.
3. The stdint audit is answered by the existing `lang_scalar` table
   plus a test asserting every stdint spelling it names maps at a
   pair site (one table, as the request hoped).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.

## 28. R6 — string-view fields in boundary structs

Owner decision 2026-08-01 (downstream report R6, blocking; also a
**bug report accepted as such**): a `string`-typed field in a
pointer-passed boundary struct mirrors today but the shared lowering
places the string handle in the struct storage instead of the
16-byte `{data,len}` view — every later field reads at a wrong
offset in C, identically in both tiers, so the differential gate
cannot see it. That is accept-and-miscompile against invariant 1.

### 28.1 Rules

1. **No accept-and-miscompile, anywhere.** The mirror may accept a
   string field only where the lowering below exists; every other
   position — by-value boundary-struct crossings with string
   fields, arrays of such structs, and any aggregate position not
   listed — fails loud at bind time with the §13-style named
   construct. An audit test enumerates the mirror's string-field
   positions and asserts accepted ⇒ lowered.
2. **R6.1 — script → C (pointer-passed).** At the call site, both
   tiers build a C-layout scratch struct: the string field expands
   to the view `{const char* data; size_t len}` pointing at the
   language string's bytes — zero-copy, valid for the duration of
   the call (the parameter-position string-view rule at field
   granularity); remaining fields copy at their C offsets. The
   callee copies if it retains, as with parameter views.
3. **R6.2 — C → script.** Reading a `string`-typed field from a
   C-filled struct materializes a language string from the stored
   view (the existing view-to-string copy-in, as the callback
   trampoline does for messages); an all-zero view reads as the
   empty string. May land with R6.1 or immediately after — the
   fail-loud rule covers whichever direction is not yet lowered.

### 28.2 Corpus and staging — Red first

The fix starts by pinning the bug: `a97` (accept) constructs a
descriptor-shaped fixture struct — string-view field first, scalar
fields after it — passes it to a C checker that returns the scalar
fields and the viewed bytes, and prints them. Under the pre-fix
compiler this entry's golden CANNOT be captured correctly (the
scalars come back wrong); the entry lands **with** the fix and its
golden proves the offsets. The read direction adds a C-filled record
carrying a message view (`a98` or folded into `a97` — implementer's
call, contract requires both directions pinned). The fixture gains
the checker/filler; mirror regenerated via `subscript bind`.

### 28.3 Exit criteria (pre-registered)

1. `a97` (and the read-direction pin) byte-identical under both
   tiers; the scalar-after-string values prove the C offsets.
2. The audit test: every mirror-accepted string-field position has
   a lowering; by-value structs and arrays of string-field structs
   fail loud at bind time, unit-tested.
3. The downstream shape — `SGPUStringView label` first, `uint64_t`
   fields after — is covered verbatim by the fixture struct.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.

## 29. AI-facing generated reference

Owner decision 2026-08-01. Coding agents need the **current state**
of the language, not its decision history; `specs/blocks` is
history-organized by design and stays that way. The answer is
derived documents beside the §17 API reference — generated, never
hand-edited, with the same regeneration gate — plus one small
hand-written entry point.

### 29.1 `generated-docs/language-reference.md`

One generator command produces the whole `generated-docs/` set. The
language reference contains, in order: a compact current-state
surface summary (curated prose blocks embedded in the generator
source — single-sourced and code-reviewed, the §17.1 discipline);
every `RuleCode` with its `explanation()` and a **machine-excerpted
minimal rejection** — the pinned line (with two lines of context)
from the reject-harness table's (file, code, line) entry for that
code, plus the entry's header-comment guidance where present; every
`WarnCode` likewise from the warn pins; and per-feature sections
(sized numerics, value and reference classes, Q33 descriptors, Q32
literal unions, Q34 async, modules, coroutines, the memory model)
each linking its corpus entries. Codes with multiple pinned entries
pick the first; a code with no corpus pin fails the generator loud.

### 29.2 `generated-docs/corpus-index.md`

A generated table per corpus arm (accept, reject, warn, trap) from
the entries' structured header comments — name, `purpose:`,
`exercises:`, `questions:` — with the expected-output file beside
each accept/trap entry. An entry missing the header fields fails
the generator loud, which turns the existing comment convention
into an enforced one.

### 29.3 `llms.txt`

Repo root, hand-written, small and stable: the read order
(language-reference → api-reference → corpus-index), the working
loop (`subscript check` renders each error with its rule text;
warnings carry W-codes; `subscript run` executes under the dev
tier), and the instruction **not** to read `specs/` for
current-state answers (it is the decision history). Tutorials are
listed as secondary, human-paced material.

### 29.4 Exit criteria (pre-registered)

1. A gate test regenerates both generated files and byte-compares
   them to the committed copies (the §17.4 pattern).
2. Every `RuleCode` and `WarnCode` appears with a corpus-backed
   excerpt; the generator fails loud on a code without a pin or an
   entry without headers.
3. The index covers every entry of all four corpus arms.
4. Full gate and `tsc` gate green; no golden moves; the generator
   runs offline.
5. `llms.txt` names the read order and the check/run loop.

## 30. R7 — nested aggregates beside string fields; struct-level scalar/enum pairs

Owner decision 2026-08-01 (downstream request R7, blocking its
textures area). Two gaps, one shared principle: **a mirror is either
lowered or loud** — §28.1's rule, restated because R7.2 shows a
third failure mode beside miscompile and fail-loud: a *misleading*
mirror (a leaked count field and a `const Enum*` array mirrored as
`Enum | null`) that type-checks and means the wrong thing.

### 30.1 R7.1 — aggregate fields in the §28 scratch construction

The §28 C-layout scratch extends to structs that mix string-view
fields with embedded boundary aggregates: an aggregate field copies
verbatim at its C offset (recursively — nested aggregates keep
their own layout), string fields expand to views as §28, scalars
copy as today. Script→C is the blocking direction and lands now;
the read direction gets the same treatment (aggregate copy-back
beside string materialization), and if the implementer finds it
disproportionate it may land immediately after — the §28 fail-loud
holds for whichever direction is not yet lowered.

### 30.2 R7.2 — struct-level count-first pairs, scalar and enum elements

The struct-level `<name>Count`/`<name>` adjacency rule (which
already collapses registered-struct descriptor pairs) extends to
`lang_scalar` scalars and registered u32-enum elements: the pair
mirrors as `<name>: T[]`, count elided, with the §27 semantics per
direction. And the hard rule: **any struct-level count+pointer
adjacency that does not collapse fails loud at bind time** — the
leaked-count shape from the downstream evidence must be impossible
to emit. The §28.1 audit test extends to pair positions: every
mirror-emitted array field is a collapsed pair, every uncollapsed
adjacency is an error.

### 30.3 Corpus

`a99` (accept): the downstream texture-descriptor shape verbatim —
`label` string-view field, an embedded all-scalar aggregate
(extent), a `viewFormats` enum pair, and trailing scalars — passed
script→C to a fixture checker returning every component; golden
proves the offsets and the pair collapse together. The read
direction, when it lands, extends `a99` or adds `a100`. Bindgen unit
tests pin the enum-pair collapse, the scalar-pair collapse at struct
level, and the fail-loud for uncollapsed adjacencies.

### 30.4 Exit criteria (pre-registered)

1. `a99` byte-identical under both tiers; the returned components
   prove aggregate offsets beside an expanded string field and the
   collapsed enum pair in one struct.
2. The downstream evidence shape (`SGPUProbeTextureDescriptor`)
   mirrors as `viewFormats: SGPUProbeFormat[]` with no count field;
   an uncollapsed adjacency (e.g. non-adjacent count) fails loud —
   both bindgen-unit-tested.
3. The extended audit test: every emitted array field is a collapsed
   pair; no mirror emits a bare count + pointer-as-nullable shape.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.

## 31. R8 — opaque handles in aggregate positions

Owner decision 2026-08-01 (downstream request R8, blocking its
bind-group area).

### 31.1 R8.1 — handle elements in count-first pairs

The pair-element set (§30.2) extends to registered opaque handles:
an adjacent `size_t <name>Count, const H* <name>` where `H` is a
registered handle typedef collapses to `<name>: H[]`, count elided —
input direction only in v1 (a mutable handle-array direction is a
new request with evidence). Elements are pointer-sized values; the
language array's storage crosses as the `const H*`.

### 31.2 R8.2 — nullable handle fields via `_Nullable`

The mirror honors clang's `_Nullable` qualifier on **handle fields
of boundary structs**: the field mirrors as `H | null`; script
`null` lowers to `NULL`, and on the read direction a `NULL` field
reads as `null`. The unqualified handle field stays non-null, as
today. Chosen over a provenance directive because the frontend is
already libclang, the header stays self-documenting, and the
downstream facade header is generated and can emit whatever spelling
the contract names.

The silent-ignore mode the probe found is closed the §30 way:
`_Nullable` on any position without this lowering — non-handle
fields, parameters, returns, pair elements — **fails loud** naming
the position, so a qualifier the mirror does not honor can never be
dropped silently again.

### 31.3 Corpus

`a101` (accept): the pipeline-layout shape verbatim — label view +
handle-element pair — passed script→C to a fixture checker that
returns per-element identity evidence. `a102` (accept): a
bind-group-entry shape with three `_Nullable` handle fields, script
setting exactly one non-null per entry, the checker reporting which;
plus the read direction (C fills one of three). Bindgen unit tests:
handle-pair collapse; `_Nullable` handle field mirrors `H | null`;
`_Nullable` on an unsupported position fails loud.

### 31.4 Exit criteria (pre-registered)

1. `a101`/`a102` byte-identical under both tiers.
2. The downstream evidence shapes mirror exactly:
   `bindGroupLayouts: SGPUBindGroupLayout[]` (no count), and a
   `_Nullable` handle field as `H | null` — bindgen-unit-tested.
3. `_Nullable` anywhere unsupported fails loud (unit-tested); the
   §30 audit still holds (every array field a collapsed pair).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.

## 32. R9 — recursive lowering at embedded positions

Owner decision 2026-08-01 (downstream request R9, blocking its
pipeline area; this is §30.1's recorded boundary arriving with its
evidence). The generalization: **"recursively plain" widens to
"recursively lowered"** — the absorbed lowerings that exist at top
level apply at embedded positions.

### 32.1 The rule

In the script→C scratch construction, an embedded boundary
aggregate is built by the same rules as its parent, recursively:
string-view fields expand to views (§28), collapsed count/pointer
pairs emit `(count, ptr)` from the language array (§30/§31,
including handle elements), `_Nullable` handle fields lower `null`
to `NULL` (§31), plain scalars and plain aggregates copy at their C
offsets. The same recursion applies at **pair-element positions**:
a pair whose element type is itself a lowered boundary struct
crosses as a scratch **array** — each element built recursively —
valid for the duration of the call, like every §28 view.

Direction: script→C, per the evidence (no read-direction case
blocks). Reading any recursively-lowered position stays fail-loud —
the §28/§30 discipline — until evidence arrives. Whatever a
recursive position still cannot lower (an absorbed member with no
write-direction lowering of its own) stays fail-loud naming the
innermost offending member.

### 32.2 Corpus

`a103` (case 1): the compute-pipeline shape — descriptor embedding
an aggregate whose `entryPoint` is a string view. `a104` (case 2):
the render-pipeline depth chain composed end to end — descriptor →
vertex-state aggregate → buffers pair → buffer-layout element →
attributes pair — pinning the composition, not just the isolated
primitives. `a105` (case 3, may lag per the HANDOFF): a pair whose
elements are string-field structs (`constants` entries); if it
lags, its fail-loud stays and the lag is recorded in tracking, not
silent. All script→C through fixture checkers returning every
component; goldens by the standard capture path.

### 32.3 Exit criteria (pre-registered)

1. `a103`/`a104` byte-identical under both tiers; `a104`'s checker
   proves values across the full depth chain.
2. The three verbatim evidence rejections at pin `2016bf0` now
   mirror and lower (bindgen-unit-tested on those shapes), except
   `a105`'s if it lags — then its fail-loud text is pinned instead.
3. The §30 audits extend through recursion: every emitted array
   field is a collapsed pair and every accepted string-field
   position has a lowering, at any depth.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.

## 33. R10 — lowering through struct-pointer members

Owner decision 2026-08-01 (downstream request R10, the last §32
recursion axis; blocking its render pipelines). §32's reachable set
covers by-value embedded aggregates and pair elements; a lowered
struct behind a **struct-pointer member** was not reachable.

### 33.1 The rule

Reachability extends through struct-pointer fields, recursively,
starting from a direct foreign descriptor-pointer parameter: a
boundary struct reachable only through `[nullable]` struct-pointer
members gets the same recursive script→C lowering as §32. At the
scratch construction, a non-null struct-pointer field lowers to a
pointer to a recursively-built scratch struct (call-duration valid,
like every §28 view and §32 scratch array); `null` lowers to
`NULL`. The read direction stays fail-loud, diagnostics naming the
innermost member (§32 discipline). The §30/§32 audits extend over
the pointer-reachable set: lowered-or-loud at any depth through any
mix of embedding, pair elements, and pointer members.

### 33.2 Corpus

`a106` (accept): the full render-pipeline composition at the
downstream depth — descriptor → nullable `fragment` pointer →
fragment state (string view + constants pair + `targets` pair whose
elements carry a nullable `blend` pointer to a plain struct) — with
both the null and non-null spellings exercised and a checker
returning evidence from every level including behind the pointers.
Goldens by the standard capture path.

### 33.3 Exit criteria (pre-registered)

1. `a106` byte-identical under both tiers, covering null and
   non-null pointer fields and the values behind them.
2. The verbatim evidence rejection at pin `aeaffcf` now mirrors and
   lowers (bindgen-unit-tested on that shape); pointer-reachable
   structs whose members still lack a lowering fail loud naming the
   innermost member.
3. The audits hold over the pointer-reachable set (extended
   `..._at_any_depth` coverage).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.

## 34. R11 — parameter-position handle-element pairs

Owner decision 2026-08-01 (downstream request R11, blocking its
encoder area). The last cell of the pair matrix: §27 collapses
parameter pairs with scalar elements, §31 struct-level pairs with
handle elements; parameter pairs with handle elements did neither —
and mirrored the R7.2-class misleading split (leaked count, array
pointer as `H | null`).

### 34.1 The rule

Adjacent parameters `size_t <name>Count, const H* <name>` with a
registered-handle `H` collapse to `<name>: H[]`, count elided, §27
input-direction semantics; a mutable (`H*`) parameter pair fails
loud as input-only (§31.1's rule at parameter position). And the
class closes: the **"every emitted array field is a collapsed
pair" audit extends to parameter positions** — no pair position
anywhere, struct or parameter, any element kind, may mirror as a
bare count beside a nullable-value pointer.

### 34.2 Corpus

`a107` (accept): the queue-submit shape — fixture-created handles
submitted as an array beside a leading handle parameter; checker
returns the count and per-element identity evidence. Bindgen unit
tests: the collapse on the verbatim evidence signature; mutable
handle parameter pair fails loud; the extended audit.

### 34.3 Exit criteria (pre-registered)

1. `a107` byte-identical under both tiers.
2. The evidence signature mirrors
   `(queue: SGPUQueue, commands: SGPUCommandBuffer[])` — no count,
   no `| null` — bindgen-unit-tested.
3. The parameter-position audit holds; a mutable handle pair at
   parameter position fails loud.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.

## 35. R12 — `_Nullable` handle parameters

Owner decision 2026-08-01 (downstream request R12, blocking its
pass encoders). The §31.2 field rule at parameter position:
`_Nullable` on a **registered opaque-handle parameter** mirrors
`H | null`, and script `null` lowers to `NULL` at the call.
Unqualified handle parameters stay non-null. The honored-position
set grows by exactly this one entry; every other `_Nullable`
position keeps §31.2's fail-loud, and the fail-loud message's
"only ..." list is updated to name both honored positions.

Corpus: `a108` (accept) — the set-bind-group shape: a leading
encoder handle plus a `_Nullable` handle parameter, called once
with a live handle and once with `null`, the checker reporting
which. Bindgen unit tests: the collapse on the verbatim evidence
signature; `_Nullable` on a non-handle parameter still fails loud.

Exit criteria: (1) `a108` byte-identical under both tiers, both
spellings; (2) the evidence signature mirrors
`(encoder: ..., group: SGPUBindGroup | null)` — unit-tested;
(3) the §31 fail-loud suite still passes with the updated message;
(4) no existing golden moves; full gate, `tsc`, zero-warning, and
generated-docs gates green.

## 36. OBS-1 — emitted C names every referenced boundary typedef

Owner decision 2026-08-01 (downstream observation OBS-1, accepted
as a bug): a boundary class referenced **only in null position**
(every use passes `null` for its `X | null`; no construction, no
member access) was omitted from the ship tier's emitted typedefs
while the emitted call signatures still named it — an accepted
program failing late at C compilation. The dev tier, with no C
emission, was unaffected; no corpus entry had the shape, which is
why the gate never saw it.

Rule: the C emitter's type-reachability walk keys off **referenced**
boundary types — any type named by an emitted signature, field, or
element — not off constructed/accessed types. An accepted program's
emitted C compiles; "accepted" may not mean "fails later at the C
step".

Corpus: `a109` (accept) — a program whose only use of a boundary
class is passing `null` where `X | null` is expected, verified
end-to-end under both tiers (the ship path compiles and runs).
Staging is Red-first: reproduce the C compile failure at the current
pin before fixing; the entry lands with the fix.

Exit criteria: (1) the pre-fix reproduction is recorded (the C
error text); (2) `a109` byte-identical under both tiers; (3) an
emitter unit test pins that every typedef named by emitted
signatures is defined in the emitted C; (4) no existing golden
moves; full gates green.

## 37. R13 — async instance methods on reference classes

Owner decision 2026-08-02 (downstream request R13, blocking its P5
slice 1). The Q34 model gains a receiver; nothing else in §26
moves. Probed before contracting: stock `tsc` accepts
`async name(): Promise<T>` as a class method and permits a floating
async method call, so the floating rejection below is a
strictly-narrower pin (r105 `tsc`-clean). Checker probes: generic
classes and `@CStruct` value classes accept synchronous methods
today, so their async variants are explicit rejections here, not
pre-existing behavior.

### 37.1 Checker

`async name(args): Promise<T>` is accepted as an **instance method
of a plain, non-generic reference class**, with the explicit
`Promise<T>` return annotation §26.1 requires; `await` legality
inside the body is §26.1 unchanged. The await grammar gains exactly
one form: `await recv.m(...)` — a direct call of an async method
through a receiver expression of the declaring class's type
(including `this`). The receiver is evaluated once, before the
arguments.

Rejected, with corpus pins: an async static method (r101), an async
generator method (r102), an async method on a `@CStruct` value
class (r103), an async method on a generic class template (r104),
and a non-awaited async method call — floating statement or value
position (r105, S013, `tsc`-clean). Async methods on `@Descriptor`
classes stay covered by the existing methods-on-descriptors
rejection (r94). Awaiting a synchronous method is an error,
symmetric with §26.1's synchronous-function case. Async methods are
not first-class values, exactly as async functions are not.

### 37.2 Lowering (both tiers)

HIR already carries a method as a function whose first argument is
the receiver; an async method lowers as a §26.2 async function
whose frame's first slot holds the receiver reference.
`await recv.m(...)` evaluates the receiver, allocates the callee
frame with the receiver and arguments stored, and proceeds per
§26.2. The receiver persists across suspensions because the frame
does: the collect mark walk already treats live async frames as
roots, so a receiver held by a suspended frame survives an explicit
`Context.collect()`. No runtime C API change; methods are not
module exports, so the §26.3 standard-runner convention is
untouched.

### 37.3 Corpus

`a110-async-method-receiver` (accept): a reference class whose
async method awaits `Context.suspend()` and awaits a sibling async
method through `this`, mutating receiver fields across suspensions;
async `main` awaits it through an object; a second exported async
root runs `Context.collect()` while the first chain is suspended
(the a94 two-root kick), so the resumed method's prints pin
receiver survival under explicit collection.
`a111-interop-async-method-poll` (accept): the a95 foreign-poll
shape with a receiver — a class wrapping the fixture's
`subDevicePoll`, its async method polling to readiness. Rejects
r101–r105 as listed in §37.1.

### 37.4 Exit criteria (pre-registered)

1. `a110`/`a111` byte-identical under both tiers, driven by the
   standard §26.3 pump; `a111` reaches readiness through the same
   fixture counter `a95` uses.
2. `r101`–`r105` pin (code, line); `r105` type-checks under stock
   `tsc`, recorded in its header.
3. `a110`'s golden shows receiver state intact after a
   `Context.collect()` issued while the method was suspended.
4. `tsc` gate green with the new accepts included; no existing
   golden moves; full gate green; zero-warning sweep unaffected.

## 38. Workers round 1 — module state is Context state in both tiers

Owner decision 2026-08-02. First of three rounds toward the Workers
model (host-owned threads, one Context per thread, runtime message
channels; the Q35 register entry lands with the final round). This
round is the isolation prerequisite and stands on its own.

Grounding, from source: the dev tier reaches every module global
through the Context-owned block (`lower/mod.rs`'s contract — "module
globals live in a host-owned block reached through the Context";
the ABI slot is `Context::globals_offset()`, offset 16), while the
ship tier emits each module global as a process-wide C `static`
(`cemit.rs` global emission). Sequential multi-Context use never
observes the difference because `subscript_init` reinitializes, but
two **concurrent** Contexts of one program image would share and
race ship-tier module state while dev-tier state stayed
independent — a tier divergence the differential gate cannot see,
because it drives one Context. The R6 lesson applies: tier
agreement on the existing corpus is not correctness.

### 38.1 Rule

**Module state is Context state, in both tiers.** The ship tier
emits module globals as one layout-fixed block allocated per
Context during `subscript_init` and reached through the same
Context slot the dev tier uses; a `static` definition for
language-visible module state is forbidden in emitted C. Immutable
data (string literal bytes, lookup tables) stays shared. A Context
is thread-affine: created, driven, and released wholly on one
thread (§14.6's single-threaded contract, restated for the
multi-Context case); cross-thread migration of a live Context
remains uncontracted.

### 38.2 Gates

1. A concurrency harness, both tiers: one program with mutating
   module state runs in two Contexts on two OS threads
   concurrently; each Context's per-Context stdout capture equals
   the single-Context golden byte-exactly. Ship tier: one compiled
   image, two Contexts — the case today's `static` emission fails.
   Dev tier: one session per thread. Headless, deterministic (each
   thread joins before comparison; no cross-thread ordering is
   asserted).
2. An emitter unit test: emitted C for a program with module
   globals defines no `static` module-state storage.
   *(Strengthened 2026-08-02 by the arc's Clean Review: the
   name-based test pinned the probe global and missed a second
   mutable-static class — see 38.3. The gate is now an audit:
   emitted C contains no mutable `static` definition of any kind;
   immutable rodata is whitelisted explicitly.)*

### 38.3 Review finding — capturing-lambda environments (2026-08-02)

The arc's no-context review found the ship tier still emitting one
mutable-static class §38.1 forbids: every capturing lambda's
environment (`static EnvL<n> …;` at its creation site), written on
every call, shared process-wide. This was wrong before Workers in
plain single-threaded programs — a function that creates a
capturing lambda, recurses, then calls the lambda reads the
recursive call's environment (dev tier 3, ship tier 0 — verified
live by the reviewer) — and Workers make the concurrent case
routine. C5 (captures never escape their defining function) is
exactly the property that makes an automatic-storage environment
sound; the `static` was never load-bearing.

Fix contract: the environment becomes function-local automatic
storage; `a114-lambda-env-recursion` pins the recursion pattern
Red-first (the pre-fix ship divergence is recorded, then the entry
lands with the fix); the 38.2-2 audit above pins the class. Also
from the same review, runtime-side: `subscript_rt_globals_init`'s
size/align conversion-failure arms must trap before returning null
(today they return bare null and emitted `subscript_init` returns
with no trap recorded — the harness then proceeds and crashes on
the null globals slot; reachable only off 64-bit hosts, fixed
loudly anyway).
3. Existing goldens byte-identical (single-Context semantics are
   unchanged); full gate and `tsc` gate green.
4. The standing ship-tier benchmark re-measured and the ratio
   recorded in `specs/tracking/workers.md` — global access gains an
   indirection; the cost is measured and reported, not assumed.

## 39. Workers round 2 — runtime threads, channels, worker lifecycle

Owner decision 2026-08-02: Workers are **standard library**, not a
host pattern; this supersedes the CLAUDE.md sentence listing
threads among host-only capabilities (the CLAUDE.md revision and
the Q35 register entry land with round 3). Round 2 is runtime-only:
no language surface, no prelude change, no corpus change. The
round-3 script surface was probed before this contract: the
Worker/Inbox/Outbox ambient and two probe programs — including
`Worker.spawn(entry)` type-argument inference — type-check under
stock `tsc` (exit 0, recorded in tracking).

### 39.1 Model

A worker is a runtime-owned OS thread running a dedicated Context
of the same program image (§38 isolation). The spawning generated
code supplies two C function pointers: the program's module
initializer and the worker entry. The runtime creates the worker's
Context **on the worker thread**, runs the initializer, runs the
entry with the Context and its channel endpoints, and releases the
Context on that same thread — §38.1 thread-affinity holds by
construction. Messages cross as byte copies of fixed-size payloads
through two runtime-owned queues per worker (parent→worker,
worker→parent); the receiving side materializes each message as a
fresh allocation in the receiving Context (invariant 1 makes the
byte copy tier-portable). Queues are unbounded; posting never
blocks. Workers may spawn workers.

### 39.2 C API

New `subscript_rt_*` functions covering: spawn (parent Context,
initializer pointer, entry pointer, in/out payload sizes), post to
a worker, non-blocking poll from a worker, close (subsequent worker
receives observe end-of-input), join, and the worker-side endpoint
operations (blocking wait, non-blocking poll, post). Exact names
and signatures are the implementer's within these semantics; the
generated host header documents the public subset, and
generated-code-only entry points stay out of it (the
`subscript_rt_globals_init` precedent).

Failure: a trap in the worker trap-stops its entry per C6 and the
thread then ends normally. `join` on a worker whose Context trapped
**traps the joining Context** — trap kind `worker-trapped` (22) — a
worker failure is loud at the join point, never silent. `close`
then `join` is the orderly shutdown; a worker handle never outlives
its parent Context (release of the parent closes, joins, and frees
remaining workers — teardown may discard queued messages, the
§26.2 no-cleanup precedent).

### 39.3 Concurrency contract

The worker/channel module is the runtime's only shared-mutable
state (tracking records the pre-existing zero-global finding).
Blocking waits use OS synchronization (condvar), never spinning.
Every `unsafe impl Send`/`Sync` carries a `// SAFETY:` comment.
§14.6 is unchanged: each Context, worker Contexts included, is
driven only from its own thread.

### 39.4 Exit criteria (pre-registered)

1. Runtime unit tests drive a hand-written echo worker through the
   C ABI (the `test_async_resume` precedent): spawn, post N,
   worker waits and replies N, parent polls N, close, join —
   deterministic, headless.
2. A trap raised in the worker entry surfaces as trap kind 22 on
   join in the parent (unit test); a clean worker joins without
   trapping.
3. Two workers concurrently, with interleaving-independent
   assertions only (per-worker reply sets, never global order).
4. Parent-Context release with live workers closes, joins, and
   frees them (unit test; no leak under the existing allocation
   accounting).
5. Full gate green; no golden moves; generated-header byte-compare
   green; no new mutable global state outside the worker/channel
   module.

## 40. Workers round 3 — the language surface (Q35)

Owner decision 2026-08-02. The script-facing surface is
`stdlib.md` §16 (ambient `tsc`-probed, tracking); this section is
the checker/lowering contract. The example requirement is the
owner's, verbatim intent: a real parallel computation across
worker threads, not a messaging demo.

### 40.1 Checker

`Worker<In, Out>`, `Inbox<T>`, `Outbox<T>` are built-in generic
reference types, monomorphized per message-class pair like other
generics. Enforced, each with a corpus pin:

- `Worker.spawn(entry)`: `entry` is a directly named module-level
  synchronous non-capturing function of the exact entry shape. A
  capturing lambda rejects (r106 — stock `tsc` accepts it, so the
  pin is `tsc`-clean, recorded in its header); an `async` entry
  rejects (r107 — also expected `tsc`-clean via `void`-return
  assignability, verify and record).
- Message classes are transferable per stdlib §16.2; a string
  field rejects (r108, innermost-field diagnostics per the §32
  precedent).
- Context-affinity: a `Worker`/`Inbox`/`Outbox` module global,
  class field, array element, or lambda capture rejects (r109 pins
  the module-global case; one pin for the class, the checker
  covers all four escape positions with unit tests).
  *(Amended 2026-08-02, arc Clean Review: "array element" was
  implemented for the three array positions but not for container
  type arguments — a module-global `Map<i32, Worker<…>>` stored
  and retrieved a live worker, verified end-to-end. The rule as
  now written: an affine type is illegal as ANY container type
  argument — `Map` key and value, `Set` element, alongside the
  array positions. r111 pins the Map-value case; unit tests cover
  the rest.)*
- `new Worker(...)` rejects with the checker's own diagnostic
  (r110; `tsc` also rejects via the private constructor — not a
  `tsc`-clean pin, recorded as such).

### 40.2 Lowering (both tiers)

`Worker.spawn` lowers to `subscript_rt_worker_spawn` with the
program's module initializer, the monomorphized entry (adapted to
the C entry ABI), and the two payload sizes from the message
classes' C layout. Methods lower to the §39 C API;
`wait`/`poll` results are the runtime's materialized
Context-owned instances, typed as the message class; `null` maps
from the API's empty/closed results. Byte-identical across tiers
under the standing gate.

Hot reload: a swap is **Refused** while the session's Context has
live workers (reload-layer unit test; §8.2's staleness rationale —
worker threads hold the old code).

### 40.3 Corpus, example, docs

Accept: `a112-worker-echo` — one worker echo round-trip, all
printing from `main` (worker prints nothing), deterministic golden
under both tiers. `a113-worker-parallel` — two workers computing
disjoint chunks; `main` collects and prints per-worker results in
worker order after joins (interleaving never observable in the
golden). Reject: `r106`–`r110` per §40.1.

Example (owner requirement 2026-08-02): `e11-parallel-workers.ts` —
four workers each counting primes in a disjoint range of one
problem; `main` posts one range per worker, collects, prints
per-worker counts in worker order plus the total. The golden
carries no timing; the examples README row tells the reader how to
observe the parallelism (`time` on the CLI run, worker count
visible in the source). The computation is deliberately simple;
the parallelism must be real (concurrent computing workers, not
sequential round-trips).

Prelude gains the §16.1 ambient verbatim; generated-docs
regenerate (the language-reference gains a Q35 block; corpus-index
picks up the new entries).

### 40.4 Exit criteria (pre-registered)

1. `a112`/`a113` byte-identical under both tiers; `e11` runs under
   the examples harness in both tiers with its committed golden.
2. `r106`–`r110` pin (code, line); `r106` verified `tsc`-clean and
   recorded; `r107`'s `tsc` status verified and recorded either
   way.
3. Checker unit tests cover all four escape positions and the
   non-entry spawn arguments.
4. Reload refusal with live workers (unit test). *(Added
   2026-08-02, arc Clean Review m-3: plus a reload-mode worker
   echo round-trip — the reload-only init branch in the worker
   entry path runs end-to-end, not only the Refused pin.)*
5. `tsc` gate green with prelude + new corpus + `e11` included; no
   existing golden moves; full gate green; zero-warning sweep
   green; generated-docs byte-compare gates green.
6. *(Added 2026-08-02, arc Clean Review m-1)*: the reload-session
   fn-table pointer crosses to worker threads laundered as
   `usize`; the invariant that makes it sound (LiveWorkers refusal
   plus ReloadSession field order joining workers before code is
   dropped) is written as a `// SAFETY:` comment at the crossing
   and pinned by a comment-presence assertion or field-order
   test — the §39.3 SAFETY rule applies to laundered crossings,
   not only to literal `unsafe impl`.

## 41. R14 — `switch` over Q32 literal-union aliases

Owner decision 2026-08-02 (downstream request R14; not
hard-blocking — the downstream held its slice on the same-day
cadence rather than shipping an `if/else` fallback that a
102-member alias would make both slow and silently
non-exhaustive). Probed before contracting, `--lib es2022`
standalone: an exhaustive alias switch, a `default` subset, a
missing-member switch, and a duplicate-member switch are all
stock-`tsc`-clean; a non-member `case` label is `tsc`-rejected
(TS2678).

### 41.1 Checker

A Q32 string-literal union alias (§24) joins the legal `switch`
discriminant types. On an alias-typed discriminant every `case`
label must be a string literal naming a member (§24 contextual
typing; a non-member rejects — r113, `tsc`-rejected too,
recorded). Closed-set exhaustiveness: **without `default`, the
cases name every member exactly once** — a missing member rejects
(r112, `tsc`-clean, strictly narrower) and a duplicate member
rejects (r114, `tsc`-clean); with `default`, any subset of
distinct members is legal. Nothing else about `switch` changes
(break, fallthrough, scoping, other discriminant types).

### 41.2 Lowering (both tiers)

Case labels lower to their §24 `i32` discriminants; dispatch is
integer compare only — never a string comparison. The ship tier
emits a C `switch` over the discriminant (jump-table eligible);
the dev tier lowers as the existing integer switch. A cemit test
pins the a115 switch's emitted form: integer case labels, no
string-comparison call (the §24 test precedent).

### 41.3 Corpus

`a115-switch-literal-union` (accept): an exhaustive switch over a
three-member alias and a `default`-subset variant; the golden pins
the dispatch result for every member through both paths. Rejects:
`r112-switch-alias-missing-member` (no `default`, one member
absent; `tsc`-clean recorded), `r113-switch-alias-non-member` (a
label outside the set; `tsc` status recorded), `r114-switch-alias-duplicate-member`
(`tsc`-clean recorded).

### 41.4 Exit criteria (pre-registered)

1. `a115` byte-identical under both tiers.
2. `r112`–`r114` pin (code, line) with `tsc` statuses recorded in
   their headers.
3. The cemit pin of §41.2 (integer labels, no string compare).
4. Checker unit tests: missing member, duplicate member,
   non-member, `default` subset, and an exhaustive switch
   accepted.
5. Full gate green; `tsc` gate green with `a115` included; no
   existing golden moves; zero-warning sweep green;
   generated-docs byte-compare gates green.

## 42. R15 — divergence flow: exhaustive switches and `unreachable()`

Owner decision 2026-08-02 (downstream R15; 15.1 blocking, 15.2
answered as a design decision). Probed before contracting
(`--lib es2022` standalone, exit 0): the R15.1 function shape and a
`declare function unreachable(): never` ambient are both
stock-`tsc`-clean — `tsc`'s own flow analysis accepts an exhaustive
alias switch with all-returning arms and treats the `never` call as
diverging, so the two rules below only align this compiler with
what `tsc` already concludes.

### 42.1 R15.1 — exhaustive alias switches in return-flow analysis

A `default`-less `switch` over a Q32 alias (§41 guarantees the
cases cover every member) counts as **diverging** in the
all-paths-return analysis when every case arm diverges (returns, or
ends in a diverging statement per this section). No runtime or
type-system change; arms that fall through to a `break` keep the
existing behavior (the switch then does not diverge). Enum,
integer, and string switches are unchanged — they have `default`-
free coverage no analysis can prove.

### 42.2 R15.2 — `unreachable()`

The prelude gains `declare function unreachable(): never;`. The
call is a statement (value position rejects — r115); flow analysis
treats it as diverging; at runtime it traps with the new trap kind
`unreachable-reached` (23) under C6's trap-stop semantics,
identically in both tiers. This is the intended spelling for
provably-dead paths in generated code — the bounds-check idiom the
downstream considered is not the designed answer. Dead-code
detection after a diverging statement is not added (existing
behavior unchanged).

### 42.3 Corpus

`a116-exhaustive-switch-returns` (accept): the R15.1 function shape
verbatim (every arm returns, no trailing return) plus a function
whose tail is `unreachable()` after early returns — the a115
assign-and-break concession removed. `t47-unreachable-reached`
(trap): a reachable `unreachable()` call traps with kind 23,
golden-pinned under both tiers. `r115-unreachable-as-value`
(reject): `unreachable()` in value position (`tsc` status probed
and recorded either way).

### 42.4 Exit criteria (pre-registered)

1. `a116` byte-identical under both tiers; `t47` traps with kind 23
   under both tiers, golden-pinned.
2. `r115` pins (code, line) with its `tsc` status recorded.
3. Flow unit tests: all-arms-return exhaustive switch accepted; one
   arm ending in `break` still rejects all-paths-return; a
   `default`-bearing switch unchanged; `unreachable()` satisfies
   return flow at a function tail.
4. Full gate green; `tsc` gate green with the prelude addition and
   `a116` included; no existing golden moves; zero-warning sweep
   green; generated-docs byte-compare gates green.

## 43. R16 — absence-capable Q32-alias descriptor members

Owner decision 2026-08-02 (downstream R16, re-sent; not blocking
but shaping the downstream's public API wrongly — its E2 shipped
WebGPU's `compare` member required, which makes every explicit
sampler descriptor a comparison sampler). Scope taken at the
downstream's offered minimum: **Q32-alias members only** — the one
value type with a spare representation for "absent" by
construction, and the place absence is semantically loaded in
WebGPU IDL. Probed before contracting (`--lib es2022` standalone,
exit 0): the declaration form, the presence test with narrowed
reads, and template prints are stock-`tsc`-clean; an explicit
`undefined` member value is also `tsc`-accepted, so its rejection
below is strictly narrower.

### 43.1 Surface

Inside a `@Descriptor` class, `name?: A` with **no initializer**,
where `A` is a Q32 string-literal union alias (§24), declares an
**absence-capable** member. In a literal, omitting the member means
*absent* — a state distinct from every member value; supplying it
means that value. Absence is only spellable by omission: an
explicit `undefined` member value rejects (r117, `tsc`-clean).
`?`-without-initializer for every other member type keeps its
existing rejection (r93 stands, now pinning that boundary).

Reads go through presence narrowing only: `expr.m !== undefined` /
`expr.m === undefined` on an absence-capable member is the **single
legal appearance of the `undefined` token in the language** (C7's
ban stands everywhere else — r13 unchanged; collisions C7 note
amended). The test narrows exactly as `!== null` narrows nullable
references — inside the presence arm the member reads as `A`; an
unnarrowed read rejects (r118 — `tsc`-clean, template positions
accept `undefined`).

### 43.2 Runtime and boundary

Absent is a reserved discriminant outside the alias's member set
(representation is the implementer's; observable behavior is the
contract). The §24 formatting string table never sees it — reads
are narrowed by construction. Both tiers byte-identical. **No
bindgen change**: the request is script→C only, and the sentinel
write at the C boundary is the downstream generator's code built
from presence tests.

### 43.3 Corpus

`a118-absence-capable-member` (accept): the sampler shape — an
absence-capable alias member beside a defaulted scalar; literals
covering present, absent-with-other-members, and `{}`; presence
tests driving both arms with narrowed prints. Rejects:
`r117-explicit-undefined-member` (`tsc` status recorded — probed
clean), `r118-unnarrowed-absence-read` (`tsc` status recorded).

### 43.4 Exit criteria (pre-registered)

1. `a118` byte-identical under both tiers.
2. `r117`/`r118` pin (code, line) with `tsc` statuses recorded;
   `r93` retained.
3. Narrowing unit tests: presence arm reads as `A`; the negative
   arm stays absent-typed (no read); reassignment through the
   member invalidates the narrowing (the null-narrowing precedent);
   `undefined` outside a presence test keeps r13's rejection.
4. Full gate green; `tsc` gate green with `a118` included; no
   existing golden moves; zero-warning sweep green; generated-docs
   byte-compare gates green.

## 44. OBS-3 — scalar handle fields beside arrays in scratch-lowered structs

Owner decision 2026-08-02 (downstream observation OBS-3, accepted
as a bug; blocking its P5 slice E4). The downstream reported a
run-time abort — `misaligned pointer dereference … address … is
0x1` inside `Context::array_len` — building a render pipeline whose
nullable `fragment` member is **present**, the pointed-to
descriptor carrying a handle, a string, and two array fields. It
type-checks; the failure is at execution, dev tier.

### 44.1 Narrowing established before contracting

The downstream disproved two hypotheses by running them (plain
in-language descriptors/arrays/nullable members; an empty array
field crossing the boundary through its shipped surface). The
reviewer added two more findings at the pin:

1. **Empty arrays are not the trigger.** A probe with the fixture's
   existing §33 shape — present nullable fragment, `constants` and
   `targets` both empty — ran clean and produced selector output.
   `a106` already covers the same shape with non-empty arrays.
2. **The fixture has no struct combining a scalar opaque-handle
   field with array pairs**, and none reached through a nullable
   pointer member. `SubProbeBindGroupEntry` carries handles but no
   arrays; `SubProbePipelineLayoutDescriptor` carries a handle
   *array*, not a scalar handle field.

What remains, and what the fix must reproduce first: a
scratch-lowered struct carrying a **scalar handle field beside
array-pair fields** (with a string field), reached through a
present `_Nullable` struct-pointer member — the downstream's
`GPUFragmentState` (`module` handle, `entryPoint` string,
`constants`, `targets`).

### 44.2 Rule

The §32 recursive-lowering rule stands and is restated for this
composition: **a scratch-lowered struct's field offsets are the C
layout's, whatever mix of handle, string, scalar-pair, and nested
pointer members it carries, at any depth and behind any nullable
pointer.** No field kind may shift another's offset; an accepted
program that crosses the boundary runs. Where a composition cannot
be lowered it fails loud at compile time (§28's "lowered or loud"),
never as a run-time dereference of a mis-read field.

### 44.3 Corpus and fixture

The interop fixture gains the missing composition: a struct with a
string field, a scalar `_Nullable` handle field, and two array
pairs, reached through a `_Nullable` pointer member of an outer
descriptor, with selector-based evidence at every level (the §33
fixture convention). `a119-interop-handle-beside-arrays` (accept)
drives it: fragment present with the handle non-null and null,
arrays empty and non-empty, and fragment absent. Staging is
**Red-first**: the reproduction runs at the current pin and its
observed failure is recorded before any fix; the entry lands green
with the fix.

If the composition reproduces no failure, that outcome is reported
as-is and the exact downstream program is requested rather than
guessed at — a fixture that does not reproduce is evidence, not a
fix.

### 44.4 Exit criteria (pre-registered)

1. The pre-fix observation is recorded verbatim (the failure, or
   the fact that the composition ran clean).
2. `a119` byte-identical under both tiers, covering handle
   non-null/null × arrays empty/non-empty × fragment present/absent.
3. A lowering-level unit test pins the field offsets of the new
   composition against the C layout (the `offsetof` convention).
4. No existing golden moves; full gate green; `tsc` gate green;
   bindgen regeneration byte-compare green.

### 44.5 Round 2 — the remaining axis (2026-08-03)

`a119` did not reproduce (§44 tracking). The downstream then
reported that **both** tiers fail (dev-JIT aborts in
`Context::array_len`; ship-C-AOT takes SIGSEGV), with no program
output first, and supplied its mirror and generated conversion.
The reviewer reproduced six further construction shapes against the
existing fixture — helper-returned struct temporaries as
constructor arguments, `push`-built element arrays, string-bearing
elements, null and non-null pointer elements in one array, a
defaulted array member taking its default, and the maximal
combination — **all clean under the capture harness**.

One structural axis remains untested, visible only in the
downstream's C facade: the struct behind the nullable pointer
**inside an array element** is itself an aggregate of nested
structs (`SGPUBlendState { SGPUBlendComponent color, alpha; }`),
where the fixture's counterpart holds two scalars. §32's recursion
is pinned at other positions but never at this one: array element →
nullable pointer → struct → nested struct.

The fixture gains that depth; `a120-interop-nested-behind-element-pointer`
drives it Red-first with the same null/non-null and empty/non-empty
coverage. If it too runs clean, the fixture axis is exhausted and
the next request is the downstream's **preprocessed C facade
declarations** for these structs — the one artifact not yet
supplied, and the only remaining place the shapes can differ.

### 44.6 Round 3 — the difference is the unmarked reach-through pointer

The downstream supplied its preprocessed C declarations
(2026-08-03). Its measured layouts match what the fixture already
exercises — `SGPUColorTargetState` is 24 bytes with the pointer at
+8 after a 4-byte enum and its hole, and `SGPUConstantEntry` is a
`SGPUStringView` beside a `double`, both shapes the fixture
reproduces field-for-field. Layout is therefore **not** the
difference, and the enum/alias sizing question the reviewer raised
is answered: no.

The difference is a spelling. The downstream states, and its header
shows, that `_Nullable` appears **only on opaque-handle members**;
its reach-through struct-pointer members are written **plain**
(`const SGPUBlendState* blend;`), because Q13 already mirrors a
struct pointer as `X | null`. Every reach-through pointer member in
this project's fixture carries `_Nullable`; every *plain* struct
pointer in the fixture is the data half of a count/pointer pair.
**A single, unmarked, count-less struct-pointer member has never
been exercised** — in an array element or anywhere else.

Rule: **the reach-through lowering (§33) keys off the member's
shape — a count-less pointer to a registered boundary struct — not
off its `_Nullable` spelling.** Nullability annotation is
mirror-typing information (Q13), not a lowering trigger; a mirror
that types a member `X | null` and a lowering that rebuilds it must
agree by construction, or the boundary accepts and miscompiles
(§28's rule; the R6 lesson restated for the annotation axis).

Corpus: `a121-interop-unmarked-reach-through` (accept), Red-first —
the fixture gains an array element type whose reach-through pointer
member is written plain, matching the downstream header exactly,
and the observed pre-fix behavior is recorded before any fix. If a
plain member is genuinely unlowerable, it fails loud at compile
time (§28) — never at run time.

Exit criteria: (1) the pre-fix observation recorded; (2) `a121`
byte-identical under both tiers; (3) a bindgen audit that no
registered struct-pointer member reaches the mirror without a
lowering, whatever its nullability spelling — the class, not the
instance; (4) no existing golden moves; full gates green.

### 44.7 Round 4 — two simultaneously-present pointer members

The downstream's bisection (2026-08-03) reframes the search. Its
matrix, each row a separate run: every configuration with **one**
nullable struct-pointer member present runs; the pair
`depthStencil` + `fragment` present **together** aborts, and keeps
aborting as blend, vertex buffers, and the layout handle are
removed. A by-value third member does not substitute for the
second pointer.

Then, with that pair fixed, varying only a semantically unrelated
**by-value** member flips the outcome non-monotonically: omitted
aborts, `{}` aborts, `{topology}` runs, `{topology,
stripIndexFormat}` aborts. Label length does not matter, so it is
not total bytes. A neighbouring member's *contents* changing the
outcome is the signature of a **scratch sizing or indexing error**,
not of an unsupported construct — and it explains why three
purpose-built fixtures passed: the fault needs a particular scratch
profile to surface.

Confirmed at the pin: **no fixture struct carries two or more
count-less reach-through pointer members.** Every §33 shape has
exactly one; the arc varied the pointer target's depth, never the
number of simultaneously-present pointers.

Rule: **the scratch construction is correct for any number of
simultaneously-lowered members.** Each lowered position owns
storage that no other position can reach, whatever its neighbours'
kinds, contents, or sizes; scratch identity is never a function of
a sibling's payload. This is §32's recursion rule extended from
depth to breadth.

Corpus: `a122-interop-two-pointer-members` (accept), Red-first —
an outer descriptor with **two** count-less reach-through pointer
members present at once, each target itself containing a nested
aggregate and an array pair, with a by-value member between them
whose contents vary across the entry's constructions (the
downstream's `primitive` axis: absent, empty-equivalent, one field
set, two fields set). If the entry does not reproduce, the
scratch-profile axis is driven directly instead: a unit test over
descriptors with 1..N simultaneously-lowered members asserting
every position's storage is disjoint.

Testability defect, fixed in the same round: the JIT and C-AOT run
helpers buffer program output in memory and return it only on
success, so an aborting run loses everything it printed. The
downstream's "no output before the fault" was an artifact of this,
and it cost this investigation a wrong inference. Output already
produced must survive an aborting run in both helpers, with a test.

Exit criteria: (1) the pre-fix observation recorded; (2) `a122`
byte-identical under both tiers, or the disjointness unit test
standing in its place with the non-reproduction recorded; (3) the
run helpers surface partial output on abort, unit-tested; (4) no
existing golden moves; full gates green.

### 44.8 Round 5 — scale, and the hard-termination output gap

The downstream dropped its failing program and generated API layer
into the tree (2026-08-03, untracked evidence files) and reported
three facts obtained with §44.7's output fix.

**Located.** The run now prints three lines and ends at the
`createRenderPipeline` call — the fault is inside that conversion
and call, as its matrix said.

**Visible in code.** The generator's both-present arm passes
**seven** constructor arguments, two of which are separately built
aggregates; every other arm builds at most one. The descriptor
lowered by the following foreign call therefore carries, in one
tree: a string, a nullable handle, a nested state with array pairs
whose elements carry their own pairs, two by-value states, and two
reach-through pointer members whose targets themselves hold array
pairs with pointer-bearing elements. That is far more
simultaneously-lowered positions than any entry or test here has
built — §44.7's direct harness stopped at **six**.

**Mode-sensitive.** Hoisting the two aggregates into locals does
not remove the failure but changes how the run ends; adding more
locals changes it again. Combined with the earlier
`primitive`-contents flip, three independent observations now say
the behavior depends on the scratch profile's size and shape.

Rule (extending §44.7 from a small N to any N): **the scratch
construction is correct at any number of simultaneously-lowered
positions and any nesting depth, and no position's storage,
address, or size depends on how many siblings precede it.** The
existing 1..6 harness is raised to a scale that covers the
downstream's tree, and the two axes are exercised together rather
than separately.

Corpus and tests: `a123-interop-wide-descriptor` (accept),
Red-first — a single foreign call lowering a descriptor with the
downstream's profile: string, nullable handle, nested aggregate
with pointer-bearing array elements, by-value aggregates on both
sides, and two reach-through pointer members present at once whose
targets each hold array pairs. The direct harness is raised from
six positions to at least thirty-two and gains combined
breadth × depth cases.

**Testability, second defect (downstream Fact 4).** §44.7's fix
delivers output when a run ends through the non-unwinding panic
path but not when it ends by a hard signal — including lines
completed much earlier. Output already produced must survive
**however** a run ends, on both tiers, with a test per termination
mode.

Exit criteria: (1) the pre-fix observation recorded; (2) `a123`
byte-identical under both tiers, or the raised harness standing in
its place with the non-reproduction recorded; (3) output survives
each termination mode, unit-tested; (4) no existing golden moves;
full gates green.

### 44.9 Round 6 — module size decides observability

The downstream demonstrated the axis rather than suspecting it
(2026-08-03). Taking a variant that runs cleanly and appending N
functions of the form `function padN(v: u32): u32 { return v + N; }`
to the module — called by nothing, touching nothing — flips the
outcome, non-monotonically: baseline runs, +20 runs, +40 ends
early, +60 ends early, +80 runs, +100 ends early. This is the third
independent non-semantic knob, alongside the `primitive`-contents
flip and the termination mode moving between panic and hard signal
when arguments were merely hoisted into locals.

Conclusion adopted: **module size and layout decide whether the
fault is observable.** Five faithful small fixtures passed for this
reason, and no further small fixture will settle anything.

Rule: **a program's boundary lowering does not depend on the
module's unrelated content.** Adding declarations that nothing
calls cannot change the behavior of an unrelated foreign call, at
any module size.

Two lines of work, both required:

1. **Run the downstream's program here.** It dropped four files as
   untracked evidence — the failing program, its generated API
   layer, the facade header, and the generated mirror. A stub `.c`
   satisfying the header suffices, because a lowering fault does
   not depend on what the C functions do. This replaces
   reconstruction with execution.
2. **A self-contained entry for the class**: a program exercising
   a descriptor with two simultaneously-present reach-through
   pointer members, swept over a padded module of uncalled dummy
   functions (N = 20…120, step 20). The corpus entry pins the
   outcome as invariant across N; the sweep itself belongs in a
   test harness rather than in one golden.

**Output retention, scope defect (downstream round-6 datum).**
§44.8's retention is gated on an environment variable naming a
parent-owned file, so an embedder calling the run helpers directly
— which is how the downstream drives its runs — still loses
everything on a hard signal. Retention must be the default
behavior of the run helpers, without opt-in, and must be reported
as part of the run's error value rather than left for the caller
to discover.

Exit criteria: (1) the downstream program built and run here, with
its observed outcome recorded verbatim whether or not it
reproduces; (2) the padding sweep run and its results recorded;
(3) if a defect is found, fixed with the class pinned; (4) run
helpers retain output on hard termination with no opt-in,
unit-tested; (5) no existing golden moves; full gates green.

### 44.10 Retention needs address-space isolation (2026-08-05)

§44.8 criterion (3) and §44.9 criterion (4) require output retention on
both tiers. Neither states a platform limit. One limit exists.

**The ship tier retains output on every platform.** `run_c_aot*` builds
an executable and runs it as a child process. The parent reads that
child's output after any termination. No host address crosses the
process boundary, because the child links the native library's C
sources.

**The dev tier retains output only where `fork` exists.** The dev tier
runs JIT code that calls caller-supplied native symbols. `NativeLibrary`
holds each symbol as an address in the caller's process
(`codegen/src/native.rs`). A fresh process cannot resolve such an
address. Only a child that inherits the address space can. `fork` gives
that inheritance, and Windows has no equivalent. On a non-Unix host the
dev tier therefore runs the program in the caller's process. A program
that ends its own process ends the caller. No output survives, and
`RunError::AbnormalTermination` is unreachable there.

This limit is structural. While a native symbol is an address in the
caller's process, no later work removes the limit.

Both criteria now read: **output survives each termination mode on each
tier that isolates the run.**

**Consequence for tests.** A test that asserts dev-tier retention must
obtain its run through one shared helper. That helper's return type must
express "this configuration does not isolate the dev run". A call site
that ignores that case must not compile. The rule and its reason are
§11c.3's: a copied guard is a forgotten guard. Measured on
`x86_64-pc-windows-msvc` 2026-08-05, `cargo test -p subscript-codegen
--test native_library` killed its own harness with
`STATUS_STACK_BUFFER_OVERRUN` (exit `0xc0000409`); two tests ended the
harness process and one failed by assertion.

A test that covers both tiers must keep its ship-tier assertion on every
platform. The exclusion applies to the dev-tier part alone. On a Unix
host the helper supplies the run for every test, and the tests compare
exactly what they compare today.

## 45. R18 — contextual typing for conditional expressions

Owner decision 2026-08-03 (downstream request R18, non-blocking).
The request asked whether a boundary aggregate may be named as
`T | null` in script positions, and — if not — for "any way to
build one constructor argument conditionally without restating the
call", because an optional aggregate member currently forces the
generator to emit 2^n constructor calls for n such members.

### 45.1 The naming restriction is deliberate

A boundary aggregate declared by `subscript bind` is a **value
class**: C layout, copy on assign, copy on pass (C2). C7 admits
`Struct | null` only at boundary positions, where `null` has a
defined lowering — the zeroed struct for a by-value sub-layout, or
`NULL` for a §33 reach-through pointer member. In a script local,
parameter, or return type there is no such representation: a value
class has no pointer to be null. Naming it would mean giving value
types a nullable representation in script, which is a semantic
addition, not a checker oversight — the same reason C2 still defers
nullable fields *inside* value classes. Handles differ because they
already are pointers, which is why the generator's
`toNullableSGPUBuffer` helper shape is legal for them and stays so.
The restriction stands; `is_reference_shape() && !is_value_class()`
remains the rule.

### 45.2 The cost is removable, and the gap is general

The alternative the request named is the right one, and the reason
it does not work today is a defect wider than the request.
`check_cond` passes the contextual type to both branches but then
takes the **then branch's** type as the conditional's type and
requires the else branch to be assignable to it. So

```ts
const c: C | null = flag ? new C() : null;
```

is rejected for an ordinary reference class — measured at the pin,
`tsc`-clean — and swapping the branches merely swaps which side is
reported. Every `X | null` conditional in the language is affected;
the boundary case is one instance.

Rule: **when a conditional expression has a contextual type, that
type is the conditional's type and each branch is checked against
it.** With no contextual type the existing rule stands (the else
branch must be assignable to the then branch's type). Nothing else
about conditionals changes.

This removes the request's 2^n without widening C7: one constructor
call with n conditional arguments, `cond ? toSGPULimits(x) : null`
in a boundary parameter position, where `Struct | null` is already
legal.

### 45.3 Corpus

`a124-contextual-conditional` (accept): nullable reference class,
nullable handle, and — through the interop fixture — a nullable
boundary aggregate supplied as a conditional constructor argument,
in both branch orders, with the null and non-null paths both
observed. Reject: `r119-conditional-without-context` pins that an
uncontextualized conditional over mismatched branch types is still
rejected (`tsc` status recorded), and the existing S011 pins for
naming a value-class union stay as they are.

### 45.4 Exit criteria (pre-registered)

1. `a124` byte-identical under both tiers.
2. `r119` pins (code, line) with its `tsc` status recorded; no
   existing reject entry changes meaning.
3. Checker unit tests: contextual conditional in each branch order;
   nested conditionals; no-context conditional unchanged; a
   conditional whose branches are both assignable to the context
   but not to each other.
4. `tsc` gate green with `a124`; no existing golden moves; full
   gate green; zero-warning sweep green.

## 46. R19 — narrowing flows into conditional arms

Owner decision 2026-08-03 (downstream request R19; blocking the
generator change §45 was meant to enable). §45 gave the conditional
its contextual type but not the flow facts its condition
establishes, so

```ts
function viaIf(v: C | null): u32 {
  if (v !== null) { return use(v); }        // accepted
  return 0;
}
function viaCond(v: C | null): u32 {
  return v !== null ? use(v) : 0;           // rejected, S005
}
```

differ on a plain reference class — reproduced at the pin,
`tsc`-clean. The `if` form narrows; the conditional arm does not.

Recorded, because it is the second instance of the same review
gap: §45's corpus exercised the conditional's *shape* but every
case constructed its value inline, so no case reached a nullable
**local** — exactly as the OBS-3 corpus exercised descriptor shapes
but never a *returned* descriptor. A corpus entry pins a
construction, and a construction is not a flow.

### 46.1 Rule

**A conditional expression's arms are checked under the narrowing
its condition establishes**: the then arm under the facts the
condition proves, the else arm under their negation — the same
facts, from the same analysis, that the `if` statement already
applies. No new narrowing analysis is introduced; the existing
`narrow_paths` result is applied at one more site. The narrowing
does not outlive the conditional: a path narrowed inside an arm is
not narrowed after the expression.

### 46.2 Corpus

`a125-conditional-arm-narrowing` (accept): the `viaIf`/`viaCond`
parity above for a reference class, an opaque handle, and — through
the interop fixture — the generator's real shape,
`x !== null ? toX(x) : null` supplying a nullable boundary
aggregate argument from a nullable local, in both condition orders,
with both paths observed. `r120-narrowing-escapes-conditional`
(reject): a path narrowed inside an arm used after the conditional
still rejects (`tsc` status recorded).

### 46.3 Exit criteria (pre-registered)

1. `a125` byte-identical under both tiers.
2. `r120` pins (code, line) with its `tsc` status recorded; no
   existing reject entry changes meaning.
3. Checker unit tests: both condition orders; nested conditionals;
   a conditional inside an `if` arm and the reverse; narrowing
   invalidated by assignment inside an arm.
4. `tsc` gate green with `a125`; no existing golden moves; full
   gate green; zero-warning sweep green.

## 47. OBS-4 — AAPCS64 packs eightbytes, not fields (CRITICAL)

Owner decision 2026-08-03 (downstream observation OBS-4, accepted
as a miscompile). On `aarch64-apple-darwin` the dev tier delivers
**wrong values, silently**, for a by-value boundary struct with two
or more sub-64-bit integer fields; the ship tier is correct. The
callee's own print proves it is argument delivery, not computation:

| aggregate | dev-JIT receives | ship-C-AOT receives |
|---|---|---|
| `{ i32 }` | `a=3` | `a=3` |
| `{ i32, i32 }` | `x=3 y=0` | `x=3 y=7` |
| `{ i32, i32, i32 }` | `a=3 b=0 c=7` | `a=3 b=7 c=11` |
| `{ i64, i64 }` | `a=3 b=7` | `a=3 b=7` |
| `{ f32, f32 }` | `a=3.25 b=7.5` | `a=3.25 b=7.5` |

### 47.1 Cause, from this contract's own text

§12.3a records the AAPCS64 rule as "a ≤16-byte struct is packed
into registers (**its components as arguments**)". That is not
AAPCS64. B.4 passes a small non-HFA composite as **eightbyte
images** — the struct's bytes in consecutive general registers —
not one register per field. Passing components puts `x` in `x0`
and `y` in `x1`; the callee reads `x0` as the whole first eightbyte
and gets `y = 0`, and at three fields the original second field
arrives in the third position, which is exactly the matrix above.
`{ i64, i64 }` is unaffected because each field already is an
eightbyte, and `{ f32, f32 }` because an HFA is passed
component-wise in float registers, where the existing behavior is
correct.

Rule: **the dev tier's by-value marshaling follows the target
ABI's register-image rules, not its field list.** On AAPCS64: an
HFA/HVA goes component-wise in float registers; any other
composite of at most 16 bytes goes in consecutive general
registers as eightbyte images, with sub-eightbyte fields packed at
their C offsets; a larger one goes by reference to a caller copy.
§12.3a's "components as arguments" wording is corrected to this.
Win64's stated rule already packs the whole struct as one integer
and is unaffected; SysV remains a loud error.

### 47.2 Why nothing here found it

No fixture function takes a by-value struct with two or more
sub-64-bit integer fields — verified at the pin. Every by-value
aggregate the corpus passes is a `(pointer, count)` descriptor, a
string view, or a two-`i64`/HFA shape, all of which need no
packing. The downstream could not find it either: its facade
passes every struct by pointer. This is the third instance of one
pattern in this exchange — the corpus pinned the constructs it
knew, and the defect lived in a shape neither side's own code
happened to produce.

### 47.3 Corpus

`a126-interop-by-value-packing` (accept), Red-first: the fixture
gains by-value parameters covering `{i32}`, `{i32,i32}`,
`{i32,i32,i32}`, `{i16,i16,i32}`, four `u8`s, `{i64,i64}`, an HFA
of two and of four `f32`, a mixed `{i32,f32}`, a `{i32,i64}` with
its padding hole, and a >16-byte case that must go by reference —
each callee reporting every field it received, so a wrong delivery
is a wrong golden rather than a wrong sum. The pre-fix
observations are recorded before the fix lands.

### 47.4 Exit criteria (pre-registered)

1. The pre-fix matrix is recorded verbatim.
2. `a126` byte-identical under both tiers, every listed shape.
3. A lowering unit test asserts the register-image plan for each
   shape against the ABI rule, so the class is pinned and not only
   the instances.
4. §12.3a's corrected wording is the contract; no existing golden
   moves; full gates green.

## 48. R20 — external types in a generated mirror

Owner decision 2026-08-03 (downstream request R20, non-blocking).
A host header that traffics in another mirror's handles cannot be
bound: `subscript bind` either fails at the boundary use site
(`unmapped C type ... at a boundary use site`) or, via a typedef,
emits a second declaration that collides
(`duplicate class name`) or an independent brand that will not
substitute. The language already accepts the shape — a declaration
referencing a type another ambient file declares type-checks and
runs on both tiers (downstream-measured) — so only the generator
is missing it.

The existing diagnostic names two remedies, a mapping and a
typedef, and **neither exists**. That is its own defect: a
fail-loud message must name a mechanism the tool has.

### 48.1 Rule

A C type may be declared **external** to a binding: bindgen
*references* it and does not declare it. An external type is
spelled in the header itself, so `subscript bind <header>` stays
reproducible from the header alone — the §12.2 byte-identical
regeneration gate must not depend on out-of-band arguments:

```c
/* @subscript-external SGPUTextureView */
```

The emitted mirror records the provenance
(`// @subscript-c-external type="SGPUTextureView"`), references the
name at every use site, and emits no declaration for it. Resolution
is the program's: the other mirror must be among its ambient files,
and if it is not, the existing unknown-type-name error fires at the
language level — fail loud, one layer down, not a silent brand.

An external name that the header never uses is an error (a
directive that does nothing is a mistake, and this is a
generated-header convention where silence hides generator bugs).
Declaring a type external *and* defining it in the same header is
an error for the same reason.

The `unmapped C type` diagnostic is rewritten to name this
mechanism instead of the two that do not exist.

### 48.2 Corpus and gates

The interop fixture gains a second small header whose boundary
declarations use a type `interop.h` declares, marked external;
`subscript bind` produces a mirror that references without
declaring; a corpus entry binds both mirrors in one program and
crosses the boundary with a value obtained through one and consumed
through the other, byte-identical under both tiers. Bindgen tests
pin: the emitted mirror contains no declaration of the external
name; an unused external directive is an error; external plus local
definition is an error; the regeneration byte-compare covers the new
header.

### 48.3 Exit criteria (pre-registered)

1. The new fixture header binds, and its mirror declares no
   external type while referencing it at every use site.
2. The two-mirror corpus entry is byte-identical under both tiers.
3. The three error cases pin (unused directive, external plus
   definition, unresolved at the language level).
4. The rewritten `unmapped C type` diagnostic names only mechanisms
   that exist, pinned by a test.
5. Regeneration byte-compare green for both headers; no existing
   golden moves; full gates green.

## 49. R21 — a host-driven ship-tier form

Owner decision 2026-08-04 (downstream request R21). The finding
behind it is a gate-integrity one, not a feature gap: the ship
tier's runner compiles, links, and **spawns** a program whose
entry is `main`, so a host cannot run code before the script's
entry there. Every suite program therefore had to keep its
long-lived state script-side — and the downstream's P6 review
found a use-after-free that **passed both tiers precisely because
of that inversion**. The runner's shape was selecting for the
defect it should have caught.

This is the fourth instance of one pattern in this exchange
(§44.9, §46, §47): the harness exercised the constructs its own
shape permitted, and the defect lived where that shape could not
reach.

### 49.1 Rule

**A host may run its own code inside the linked ship-tier program,
before the script's entry and after its run.** The generated AOT
entry (§8.1) calls an optional host **pre-entry** function after
`subscript_init` and before `subscript_export_main`, and an
optional **post-run** function after the async pump and before the
Context is released. Both take the Context; neither is bracketed
by `enter_script`/`exit_script`, because they are host code, not
script code.

The hook names are supplied **at build time**, not discovered.
Weak symbols are not portable to `windows-msvc`, which §11c keeps
as a gated configuration, and a link-time default definition would
collide. When no hook is requested the generated entry text is
unchanged, so no existing golden moves — that is the property to
verify, not to assume.

Ownership direction is fixed: the host creates and owns; the
script borrows. Nothing here makes script-owned host state safe,
and no form is added that would.

### 49.2 Surface

The C-AOT runner gains a sibling that takes the two optional
function names alongside the native libraries; the existing
entry points keep their signatures and behavior. The golden
harness can name hooks per corpus entry, so an entry that requires
host-owned state is driven the same way on both tiers.

### 49.3 Corpus

The interop fixture gains a host-owned object with an explicit
create/destroy pair and a borrow-only accessor.
`a128-host-owned-state` (accept): the pre-entry hook creates it,
the script borrows it across **two** separate entry calls so the
lifetime spans more than one call, and the post-run hook destroys
it — byte-identical under both tiers. A companion unit test pins
that a build requesting no hooks emits the entry byte-identically
to the current text.

### 49.4 Parked, now evidenced

The downstream's preferred shape — a session API mirroring
`ReloadSession`, so one harness code path drives both tiers —
requires the ship tier to emit a **loadable library the host
process opens**, not an executable it spawns. That is the
`run --native` loader already parked on the R3 list; R21 is its
first real evidence. Recorded, not scheduled: §49 restores the
gate without it.

### 49.5 Exit criteria (pre-registered)

1. `a128` byte-identical under both tiers, with the borrowed state
   observed across two entry calls.
2. A build requesting no hooks emits the AOT entry unchanged; no
   existing golden moves.
3. Both hooks are optional and independent (pre only, post only,
   both, neither), unit-tested.
4. Full gate green; `tsc` gate green; zero-warning sweep green.

## 50. R23 — wire-mapped literal-union aliases (`CEnum`)

Owner decision 2026-08-08 (downstream request R23). Q32 (§24) barred
literal-union aliases from boundary signatures (v1), and the
downstream lowered every boundary enum to a bare integer. R23 retires
that bar for an alias that declares a wire mapping. The downstream
evidence: 45% of its generated API layer is converter functions
between union strings and mirror integers.

### 50.1 Declaration form

The prelude declares one generic alias:

    type CEnum<M extends Record<string, number>> = Extract<keyof M, string>;

`type A = CEnum<{ "m0": w0, "m1": w1 }>;` at module level declares a
Q32 alias. The members are the keys, in declaration order. Each
member carries its **wire value**: the declared integer literal.
Stock `tsc` resolves the alias to the string-literal union (measured
2026-08-08: the probe type-checks; a non-member literal fails TS2322;
a duplicate key fails TS2300).

The alias is a Q32 alias in full: §24, §41, §42, and §43 apply
unchanged. The in-language representation does not change: the
declaration-order `i32` discriminant and the per-alias string table.
The wire value is not script-visible. *(Revised 2026-08-09 by §52:
a wire-mapped alias's discriminant is the wire value itself. The
observable behavior of this section is unchanged.)*

The checker enforces three constraints beyond stock `tsc`:

- Each wire value is an integer literal in `i32` range. A fractional
  or out-of-range value is a compile error. Hex and negative
  literals are legal.
- Wire values are unique within one alias. A duplicate wire value is
  a compile error.
- The member set is non-empty.

### 50.2 Boundary positions

A wire-mapped alias is legal in a bound (mirror) signature at
parameter position and at return position. Its C type is `int32_t`.
A plain Q32 alias stays rejected in boundary signatures: the §24
rule narrows; it does not retire.

Out of scope in this slice, each parked until a downstream request:
wire-mapped aliases as boundary-struct members; emission of the form
by `subscript bind`.

### 50.3 Conversion at the crossing (both tiers)

- Parameter (script to C): the discriminant indexes a per-alias
  static table to the wire value. The wire value is passed.
- Return (C to script): the wire value maps to the discriminant. An
  unknown wire value **traps at the crossing** with a diagnostic
  that names the alias and the wire value. The trap uses the
  standing trap machinery (§18–§20) and is byte-exact across tiers.
- No string operation occurs at the crossing in either direction.

*(Revised 2026-08-09 by §52: with the wire-value discriminant, the
parameter direction is an identity pass and the table is gone; the
return direction keeps membership validation and the trap.)*

### 50.4 Corpus

The fixture is a hand-authored neutral mirror plus a committed C
callee beside the interop fixtures. `subscript bind` cannot emit the
form this slice, so this mirror is authored, not generated. Fixture
names stay synthetic (repo hygiene: no real-world API names).

- `a129-interop-wire-enum` (accept): a synthetic wire-mapped alias
  with non-dense wire values (hex, a gap, a negative). The script
  receives a value from a C return, switches on it, and passes each
  member to a C parameter that echoes the wire value. The golden
  holds the member strings and the echoed wire values, byte-exact
  under both tiers. The script contains no cast and no converter.
- `t48-wire-enum-unknown-value` (trap): a C function returns a wire
  value outside the mapping. Both tiers trap with one identical
  diagnostic.
- Reject, each `tsc`-clean (the strictly-narrower proof): `r121` a
  fractional wire value; `r122` a duplicate wire value across two
  members; `r123` a wire value outside `i32` range.

### 50.5 Exit criteria (pre-registered)

1. `a129` runs byte-identical under both tiers; the golden holds the
   member strings and the echoed wire values; the script contains no
   cast and no converter function.
2. `t48` traps with one identical diagnostic under both tiers, and
   the diagnostic names the alias and the unknown wire value.
3. `r121`–`r123` pin (code, line) and type-check under stock `tsc`.
4. A `cemit` unit test pins the crossing: parameter position lowers
   to a table access, return position lowers to a wire-to-
   discriminant mapping plus a trap path, and no string operation
   appears at the crossing.
5. Checker unit tests: a wire-mapped alias is accepted at parameter
   and return boundary positions; a plain Q32 alias stays rejected
   there; a fractional, duplicate, and out-of-range wire value are
   each rejected.
6. No existing golden moves; the bindgen regeneration gate stays
   byte-identical (bindgen untouched); full gate, `tsc` gate, and
   the zero-warning sweep are green.

## 51. R24 — `subscript bind` emits CEnum references (`@subscript-cenum`)

Owner-scheduled 2026-08-09 (downstream request R24, follow-on to
§50). §50 landed the alias but no generated mirror can carry it:
the downstream's mirrors are bind output under the byte-identical
regeneration gate, so a bound signature's type comes from the C
type alone. This section gives bind a header directive that maps a
C spelling to an alias reference. The R20 external mechanism (§48)
is the model: bind references a name it does not declare.

### 51.1 Directive

A standalone header comment, two identifiers, in the
`@subscript-external` spelling family:

```c
/* @subscript-cenum EngineFrameFormat GPUTextureFormat */
```

The first identifier names a typedef the header declares. Its base
type is `int32_t`, or it is an enum typedef. The second identifier
is the alias name the mirror will reference; it must be a legal
type identifier.

Bind then emits the alias name at every use of the typedef in a
**direct parameter or return position** of a bound function. Bind
emits a provenance comment
(`// @subscript-c-cenum typedef="EngineFrameFormat" alias="GPUTextureFormat"`)
and **no declaration** of the alias. For an annotated enum typedef,
bind emits no `declare enum` for it. The wire table stays the
downstream generator's knowledge: bind never sees the
string-to-value correspondence and never checks it.

Regeneration stays `subscript bind <header>` with no extra
arguments (§48.1 reproducibility rule).

### 51.2 Loud bind errors

Each case is a bind error that names the site:

- The named typedef does not exist in the header.
- The typedef's base type is not `int32_t` and not an enum.
- The header never uses the typedef in a direct parameter or
  return position (a directive that does nothing is a mistake).
- The typedef is used anywhere else: a struct member, a pointer
  target, an array element, or another typedef's base. Struct
  members wait for their own slice (§50.2 parked item; the
  downstream supplied shapes 2026-08-09, recorded in tracking).
- The alias name collides with any name the header declares or
  bind emits.
- A duplicate directive for the same typedef.

### 51.3 Resolution rule (the downstream's question)

The alias declaration must be an **ambient** declaration among the
program's ambient files. A module-scoped alias does not reach a
mirror. If the name does not resolve, the existing
unknown-type-name error fires at the language level (§48.1
precedent). If it resolves to a string alias without a wire
mapping, the §50 plain-alias boundary rejection fires. Bind
verifies neither: resolution is the program's, one layer down,
fail-loud.

### 51.4 Fixture and corpus

The §50.4 hand-authored mirror retires: `wire-enum.h` gains a
`typedef int32_t` spelling, the directive, and the generated
mirror joins the byte-identical regeneration gate. The alias
declaration moves to a hand-authored ambient file beside it.
`a129` and `t48` keep their goldens unchanged.

The header also gains an annotated **enum typedef** with explicit
values that match the alias's wire values, plus functions that use
it at parameter and return positions. `a130-interop-wire-enum-bind`
(accept) exercises that flavor end-to-end: a C return received, a
member passed back through a C echo, byte-identical under both
tiers.

Bindgen unit tests pin every §51.2 error case and pin that the
emitted mirror contains the provenance comment and no alias
declaration.

### 51.5 Exit criteria (pre-registered)

1. `subscript bind` reproduces both generated mirrors
   byte-identically from their headers alone; the hand-authored
   mirror is gone; `a129` and `t48` goldens do not move.
2. `a130` runs byte-identical under both tiers through the
   annotated enum typedef.
3. Every §51.2 error case pins in a bindgen unit test, each naming
   the site.
4. A checker unit test pins the unresolved-alias error for a mirror
   that references an alias no ambient file declares.
5. Full gate, `tsc` gate, and the zero-warning sweep are green; no
   existing golden moves.

## 52. Wire-mapped aliases in boundary structs

Owner-scheduled 2026-08-09, on the downstream shapes recorded in
`specs/tracking/r24-bind-cenum.md` (33 C enums, most reaching
scripts as descriptor members; the §50.2/§51.2 parked item). This
section lands the parked item and revises one §50 sentence.

### 52.1 Representation revision — the discriminant is the wire value

For a wire-mapped alias, the `i32` discriminant **is** the declared
wire value. Plain Q32 aliases keep the declaration-order
discriminant unchanged.

The reason is invariant 1: a boundary struct's memory is the C
layout, so an alias member slot must hold the wire value; an
array-pair of alias elements is zero-copy only if the elements are
wire values. (Mirror `declare enum` members already work this way:
the language `Enum` value is the C value.) One representation for
every position removes every conversion except validation where C
data enters script.

Consequences, each observably invisible:

- §24 equality and §41 `switch` stay integer compares; case labels
  and member-literal constants lower to wire values.
- §24 formatting resolves the string-table entry **by wire value**,
  never by string comparison. Every value that reaches a formatting
  site was validated where it entered, so a lookup miss is
  unreachable.
- §43.2 absence keeps its rule: the sentinel is a reserved value
  outside the member set (now outside the wire-value set), selected
  by the implementation, never observable.
- §50.3 collapses as noted there: parameter direction is identity;
  return direction keeps membership validation plus the kind-24
  trap.
- Every existing golden is byte-identical across this revision —
  an exit criterion, not an expectation.

### 52.2 Checker

A wire-mapped alias is legal in these boundary positions, beyond
§50.2's parameter and return:

- a direct member of a boundary struct;
- the element of a boundary array-pair member (§12 standalone
  descriptor and §13.2 embedded count-first pair) — the
  `viewFormats` shape;
- a constructor parameter of a mirror class.

A plain Q32 alias stays rejected in every boundary position.

Direction scope: script builds and writes these structs (the
descriptor-write path) and reads direct members back. Element-wise
readback of alias **arrays** from C stays out of scope, parked with
recursive readback.

### 52.3 Lowering (both tiers)

- Construction and member writes are plain stores: the value
  already is the wire value. Array-pair members keep the existing
  zero-copy lowering.
- A read of an alias member from a boundary struct validates
  membership and traps on an unknown value (kind 24, the §50.3
  diagnostic, at the read position). Boundary-struct slots are the
  one place C can write an alias value; validated reads keep every
  alias-typed variable a member value, which is what makes the
  formatting lookup miss unreachable.
- Reads of alias locals, parameters, and descriptor-class fields do
  not validate: their producing sites already did.
- Both tiers byte-exact under the standing gate; no string
  operation at any alias site.

### 52.4 Bind

For a §51-annotated typedef, the struct-member and embedded-pair
uses stop being errors: a direct struct member emits the alias
name; a recognized array-pair whose element is the annotated
typedef emits `Alias[]`. Every other §51.2 error case stands
(pointer target outside a recognized pair, array element outside a
pair, typedef base, collisions, zero uses, duplicates).

### 52.5 Corpus and fixture

`wire-enum.h` gains a struct carrying a direct wire-alias member
(each flavor), an embedded count-first alias pair, and plain
scalars, plus C functions that receive the struct (echo the wire
values) and fill a caller struct for readback. The generated
mirror re-joins the regeneration gate.

- `a131-interop-wire-enum-struct` (accept): script constructs the
  struct, C echoes the member and element wire values; C fills a
  struct and script reads the alias member back through a `switch`;
  byte-identical under both tiers; no converter in script.
- `t49-wire-enum-struct-unknown-member` (trap): C fills the member
  with a value outside the mapping; the script read traps; one
  identical diagnostic under both tiers.
- Boundary rejections stay checker unit tests (the §50 precedent):
  plain alias as struct member, as pair element, as constructor
  parameter.

### 52.6 Exit criteria (pre-registered)

1. `a131` runs byte-identical under both tiers; the echoed values
   prove the struct memory holds wire values.
2. `t49` traps with one identical diagnostic under both tiers,
   naming the alias and the value, at the member read.
3. **No existing golden moves** — including `a91`, `a115`, `a118`,
   `a129`, `a130`, `t48`: the representation revision is observably
   invisible.
4. `cemit` unit tests updated and added: switch case labels are
   wire values; the §50 parameter-table access is gone (identity);
   a member read emits validation plus the trap path; no string
   operation at any alias site.
5. Checker unit tests: the three new positions accepted for
   wire-mapped aliases and rejected for plain aliases.
6. Bindgen tests: struct-member and embedded-pair emission for an
   annotated typedef; the remaining §51.2 error cases retained;
   byte-identical regeneration of the extended header.
7. Full gate, `tsc` gate, and the zero-warning sweep are green.

## 53. R25 — entry-less dev sessions

Owner-scheduled 2026-08-10 (downstream request R25). The
downstream's windowed example is host-driven: the host calls the
script once per redraw, through named exports. The ship tier
supports that shape (`subscript emit --no-entry`, cli.md §2.2:
exports are the doorway, the host supplies `main`). `subscript
check` accepts the program (measured 2026-08-10: exit 0). The dev
session refused it: `ReloadSession` creation lowers the module,
and the lowering resolved an exported `main(): void`
unconditionally. The session driver does not use that entry — it
calls exports through the slot table — so the requirement was an
asymmetry between the tiers, not a protection.

### 53.1 Rule

Session creation does not require an entry point. Every
`ReloadSession` constructor accepts a module with no exported
`main(): void`. Creation still lowers the full module, runs the
module-global initializer, and applies every existing check.
`call_export` works as before. Hot reload works as before: §8.2
applies to an entry-less session unchanged.

`call_main` on an entry-less session fails loudly with the
existing `call_export` diagnostic: `` `main` is not an exported
zero-argument void function``. The failure ends the call, not the
session.

### 53.2 What does not change

- The dev and Cranelift-AOT run paths spawn a program, so they
  keep the requirement and the diagnostic
  `no exported `main(): void` entry point`.
- Ship: `emit_c` requires `main`; `emit_c_without_main` does not.
  Both are unchanged.
- A program with `main` behaves identically everywhere. Every
  existing golden is byte-identical — an exit criterion.
- The language surface does not move: the accept and reject sets
  are unchanged, and `check` already accepted the shape. No corpus
  entry — the evidence is direct unit tests (core principle 1).

### 53.3 Mechanics constraint

One model, already in the tree: the C emitter splits on
`require_main` and the run paths select the strict form. The
lowering adopts the same split — entry resolution becomes
conditional, the reload path selects the permissive form, and the
run paths keep the strict form with the unchanged diagnostic. The
exact shape (an option, an optional `Lowered` field) is the
implementer's.

### 53.4 Exit criteria (pre-registered)

1. Unit test: a module whose only exports are `frame(): void` and
   `shutdown(): void` creates a session; `call_export("frame")`
   and `call_export("shutdown")` produce the expected output.
2. Unit test: `call_main` on that session returns the §53.1
   diagnostic; a later `call_export` still works.
3. Unit test: an accepted body swap of `frame` on the entry-less
   session is observed in output.
4. Unit test: the dev run path on the same module still fails with
   `no exported `main(): void` entry point`.
5. Full gate green; every existing golden byte-identical; `tsc`
   gate green; zero-warning sweep green; `cargo fmt --check` green.

## 54. Caller link inputs follow the translation units

Downstream report 2026-08-10. The AOT link
commands put the caller's `c_sources` before the emitted
`program.c` and `entry.c`. GNU `ld` resolves an archive against
the symbols that are undefined at the position of the archive. At
that position no symbol is undefined, so `ld` keeps no member of
the archive, and the link fails on every symbol the program calls.

Measured on this host 2026-08-10 (clang 14.0.0, GNU ld 2.38): one
archive that defines `libProbe`, one caller that calls it. The
archive before the caller fails with `undefined reference to
`libProbe``. The archive after the caller links and runs. The
position of the archive is the only variable.

The defect is Linux-only. Apple `ld64` and MSVC `link.exe` resolve
an archive independent of its position, so the macOS and the
Windows gate hide it. A caller that supplies a `.c` file is also
not affected: the driver compiles that file into an object, and an
object contributes its symbols at any position.

### 54.1 Rule

Both AOT link commands order their inputs like this:

```
cc [flags] [-I…] program.c entry.c <caller c_sources> runtime.a <system libs>
```

The caller's link inputs come after the last emitted translation
unit and before the runtime archive. An include-directory argument
is position-independent, so it keeps its present position.

This applies to `run_c_aot_*` (the ship tier) and to
`run_aot_with_native_libraries` (the retained Cranelift-object
cross-check, §11).

### 54.2 What does not change

- No `--start-group`. The caller controls the order inside
  `c_sources`. A caller with a dependency between two of its own
  archives lists them in dependency order.
- The dev tier registers addresses, not link inputs (§23.6). It is
  unaffected.
- Every existing golden is byte-identical. The change moves link
  inputs; it computes nothing.
- The Cranelift-object test helper takes no native library. It
  does not change.

### 54.3 Exit criteria (pre-registered)

1. Test: a **static archive** in the `c_sources` slot, and a
   program that calls a symbol that only that archive defines. The
   test runs the ship C-AOT tier and compares the bytes to a
   committed expectation.
2. The same program runs through `run_aot_with_native_libraries`.
3. The new test fails on `1993578` on Linux. Record the red output
   before you change the source.
4. The archive fixture builds on every ship host. It must not
   spell `_Float16` (§11c constraint 2), so the MSVC gate keeps
   it.
5. `codegen/tests/native_library.rs` stays green; the workspace
   gate stays green; `cargo fmt --check` stays green.

## 55. A Cranelift frame probes the stack

Measured on `x86_64-pc-windows-msvc` 2026-08-10, toolchain 1.95.0.
`codegen/tests/boundary_scratch_breadth.rs` ends with
`STATUS_ACCESS_VIOLATION` (`0xc0000005`) in the release profile. The
dev profile passes. The defect is older than the run: a worktree at
`085ce32` fails the same way.

Cause: `dev_flags()` and `aot_flags()` set `opt_level` and `is_pic`
only. Cranelift emits no stack probe by default. Windows reserves a
thread stack and commits it one page at a time. A page becomes
committed only when the program touches the guard page that precedes
it. A frame larger than one page moves the stack pointer past the
guard page, and the first write to that frame faults.

The host profile changes no generated code. It changes how much
stack the host commits before it calls into the generated code. The
dev profile commits more, so the large frame lands on committed
pages. One profile therefore hides the defect.

The test program is the §44.8 breadth fixture: 32 positions, each
with its own target, nested state, leaf, and arrays. Its `main`
frame is large. A host program with a large frame has the same
exposure, in any profile.

Evidence, one variable, worktree at `085ce32`, release profile:

| `dev_flags()` | result |
|---|---|
| the present two settings | `STATUS_ACCESS_VIOLATION` |
| plus `enable_probestack`, `probestack_strategy = "inline"` | 1 passed |
| the two settings again, patch reverted | `STATUS_ACCESS_VIOLATION` |

`RUST_MIN_STACK=67108864` does not change the result. The reserved
size is not the cause; the missing probe is.

### 55.1 Rule

`dev_flags()` and `aot_flags()` both set:

```
enable_probestack = true
probestack_strategy = "inline"
```

The strategy is `inline` because `outline` emits a call to
`__cranelift_probestack`. The dev tier resolves a symbol by absolute
address in the caller process, and the Cranelift object goes to a
host linker. Neither one supplies that symbol.

Cranelift emits a probe only for a frame larger than one page, so a
small function keeps its present code.

### 55.2 What does not change

- The emitted-C ship tier gives the frame to the platform C
  compiler. That compiler owns the probe. This rule does not reach
  it.
- Every golden stays byte-identical. A probe writes to stack that
  the frame owns. It computes nothing.
- The rule is target-independent. Cranelift applies the setting on
  every target, and the output is the same on all of them.

### 55.3 Exit criteria (pre-registered)

1. Record the red output first:
   `cargo test --offline --release -p subscript-codegen --test
   boundary_scratch_breadth` on windows-msvc.
2. The same command passes after the change.
3. A unit test reads both flag sets back. It asserts
   `enable_probestack` is true and the strategy is `inline` in each.
4. The workspace gate passes on windows-msvc in **both** profiles.
   The dev-profile-only gate is what hid this defect.
5. The workspace gate stays green on the reference machine, and
   every golden stays byte-identical.
6. `cargo fmt --check` exit 0.
