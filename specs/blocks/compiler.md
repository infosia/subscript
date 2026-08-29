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
  tracking file when P4 opens. Criteria: ship tier within **1.5×** of
  the C baseline (eval median). Failing it reopens the backend decision
  with the measurement as the named criterion.

  *(Revised 2026-08-27, owner. This read "ship-AOT within 1.5× ...;
  dev-JIT within **4×** of the same baseline". Two things were wrong
  with it.)*

  **"ship-AOT" named a tier that no longer exists.** It meant the
  Cranelift AOT, which §11 superseded and which is now deleted. The
  ship tier is C emission and measures 1.35×, inside the 1.5×.

  **The dev-tier criterion measured the wrong thing.** Invariant 3
  states why the dev tier exists: it is "a **fast-iteration**
  development tier", and dropping it "forfeits the main
  **iteration-speed** argument for the language". §9 says the same in
  its own words — the JIT compile time "is the iteration-speed
  argument" — and then gated execution anyway. Measured on `a22`:

      dev tier   check + lower + finalize          5.0 ms
      ship tier  check + emit C, then compile+link 119.3 ms
                                                   24x faster to iterate

      dev tier   execution                        30.8x of C
      ship tier  execution                         1.35x of C

  The dev tier is 24× faster to reach a running program and 23× slower
  to run it. That is the trade the tier exists to make, and a 4×
  execution limit asked it not to make it. The limit was never met, and
  nothing re-measured it for long enough that this session found it by
  accident.

  **The dev tier's criterion is now iteration time.** Time from a
  changed source to a running program, on `a22`, must stay within
  **20 ms**, which is four times the 5.0 ms measured here. A hot reload
  of one function must stay within the same budget.

  **Dev-tier execution has a ceiling, not a target.** *(Owner,
  2026-08-27: "30× is too slow".)* It must stay within **25×** of the C
  baseline. That is a ceiling against regression, not a performance
  goal: the tier exists to iterate, and 25× is chosen from the measured
  19.6× that a constant-trip unroller reaches, with room above it.

  Making this a ceiling rather than dropping the measurement is
  deliberate. The old 4× was never met and nothing re-measured it for
  long enough that this session found it by accident; a number nobody
  can reach is not a gate. A ceiling above the measurement fails only
  when something gets worse, which is what a gate is for.

  *(No iteration-time gate existed before this. `codegen/tests/
  reload.rs` has nineteen correctness tests and measures no time. The
  property invariant 3 calls the main argument for the language was
  never measured.)*

  **The ship-tier ratio is scoped by host ISA.** *(Owner, 2026-08-28.)*
  The 1.5× held one number over two instruction sets, and the same
  emitted C does not cost the same on both. §10a records the cause,
  measured on x86-64/Windows on 2026-07-23: out-of-line growable-array
  access and copy-heavy value-class parameter passing are "both of which
  clang optimizes on arm64 but not on x86". Fix A (§10a, inline
  growable-array access) landed and moved `a22` from 17.2 ms to 14.0 ms.
  **Fix B (value-class parameters by const-pointer) was investigated and
  dropped as unsound**, and its sound restriction — leaf functions only —
  disqualifies the one value-class-parameter function in `a22`. So the
  x86-64 residual is a named and open codegen cost, not noise and not a
  port defect (`specs/tracking/windows-portability.md`).

  - **aarch64: 1.5×, unchanged.** Measured 1.34× on the reference
    machine (`specs/tracking/s70-held-async-handle.md`). This is the
    number that chose C emission over Cranelift AOT, and it does not
    move.
  - **x86-64: 2.5×.** Chosen from measurement the way §3's 25× dev-tier
    execution ceiling was chosen from a measured 19.6×: above the
    observation with room, so it fails when something gets worse. Six
    `a22` ship-tier runs on `x86_64-pc-windows-msvc` measured 2.08×,
    2.03×, 1.93×, 2.18×, 1.93×, and 1.92×.

  This is a **ceiling against regression, not a target**. Raising a
  pre-registered criterion to cover a known deficit would remove the
  only mechanism that keeps that deficit visible, which is
  `specs/tracking/gate-scope.md`'s finding with its sign reversed. The
  ceiling therefore carries the reason it is not 1.5×, and closing the
  gap is Fix B under a sound formulation — an interprocedural escape and
  alias condition, not the leaf-only restriction — which is a codegen
  change, not the backend change a 1.5× miss names.

  **The 2.5× is provisional on noise.** The run that set it reported the
  `a22` C baseline at 18.8% to 43.5% spread, over §9's ±20% limit. §9
  reports that and does not gate it, and it also means one run cannot
  pin a ceiling. A run on a quiet machine, per §9, replaces this number
  with a measured one.

- **P4 allocation gate**, and the gate becomes automatic. *(Owner,
  2026-08-28: "両方入れる".)* Two changes. `specs/tracking/gate-scope.md`
  holds the evidence and the cost.

  **The gate measured `a22` alone, and `a22` has no path to the memory
  model.** `a22` builds three growable arrays of a value type and holds
  them to the end. It frees no object and collects nothing. §68 held a
  root-set defect for 31 days, and no gate reported it. Invariant 2 is
  the memory model; a gate that measures only arithmetic cannot see it.

  **The gate now measures `collect` as well.**

      source     benchmarks/workloads/subscript/collect.ts
      baseline   benchmarks/workloads/c/collect.c, -O2, same machine,
                 same session
      subjects   C, ship tier, dev tier
      agreement  every subject computes the same i32 checksum

  Criteria:

      ship tier   within 7.5x of the C baseline
      dev tier    within 8.5x, a ceiling, as a22's is

  Derivation: the two tiers measured 6.45× and 7.04× at `1bb670d`. The
  §68 regression measured 8.07× and 10.17×. 7.5× leaves 16% over the
  measurement, and it trips 7% under the known regression. 8.5× leaves
  21%, and it trips 16% under it.

  **The gate runs from a test target.** `cargo test --release` fails
  when a threshold is missed. In a debug build the gate does not run,
  and it states the reason: §3's subject is optimized code, and a debug
  runtime is not it.

  Measured cost: `perf-gate` takes 3.96 s with the binary built, and
  `collect` adds about 7 s. `cargo test --offline --release --workspace`
  takes about 235 s. The gate is under 5% of the suite it joins.

  **A gate does not need §9's quiet machine.** §9's precision exists for
  the published comparison, where 5% is the claim. A gate reports a
  regression. `perf-gate` read 1.35× and 19.64× on a quiet machine and
  1.38× and 20.17× on a loaded one, so run-to-run variation is about 3%.
  Every limit here keeps at least 11% headroom. Noise does not trip a
  limit, and the regression this gate exists to catch was 32%.

  **`perf-gate` serves two runs, and they are not the same run.**
  *(Added 2026-08-28, after the first round removed §9's void from
  both.)*

      default    a §9 reporting run. A subject whose spread exceeds
                 ±20% has its timing withheld and the run is void,
                 exit 2. Every number this project publishes comes
                 from this run.
      --gate     a gate run. §3's thresholds decide the exit status,
                 and spread is reported and never voids. The test
                 target passes this flag.

  The two must not merge. A gate that voids on a loaded machine fails
  for a reason that is not a regression. A reporting run that prints an
  invalid timing breaks §9, which says the timing is withheld.

  If the gate run proves unstable in practice, the answer is more
  headroom or a serialized run. It is not a weaker check. Record the
  measurement that shows the instability first.

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

  *(Defect recorded 2026-08-27: the harness's default does not meet
  this rule.* `DEFAULT_WARMUP` is 3, and `a22` runs about 4 ms, so a
  default run warms up for about 12 ms against a 200 ms floor. Both
  this session and the round that deleted the Cranelift AOT tier had
  their first run declared void by the harness's own noise check, and
  both had to pass `--warmup` by hand. **A gate whose default is void
  is a gate nobody can run correctly by accident.** The floor is a
  time, so the default must reach it by measuring, not by a count.)*

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
- **Compile-time is gated, and dev-tier execution has a ceiling**:
  *(amended 2026-08-28.)* This read "compile-time is reported, not
  gated ... §3's 4× criterion is about execution". §3's revision of
  2026-08-27 deleted the 4× criterion and made iteration time the
  dev tier's criterion. Iteration time is now gated at 20 ms, and
  dev-tier execution has a 25× ceiling.
- **Both outcomes are recorded.** If a threshold fails, the tracking
  entry records the measurement, the failure, and the named criterion
  reopening the backend decision (§3) — the gate is not retried with a
  different methodology.
- **This section governs a reported measurement, not a gate run.**
  *(Added 2026-08-28.)* The quiet machine and the ±20% spread rule
  exist because a published comparison claims a 5% difference. A gate
  run under `cargo test` shares a machine with compiles and cannot
  meet them. It does not have to: every §3 limit keeps at least 11%
  headroom over its measurement, and run-to-run variation is about 3%.
  A gate reports a regression. A number this project publishes comes
  from a §9 run, and a gate result is never published as one.

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

## 11d. No emitted type has an empty member list

*(2026-08-28.)* C11 6.7.2.1 gives a structure or a union at least one
member, and 6.7.2.2 gives an enumeration at least one enumerator. GCC and
clang accept an empty one as an extension. MSVC rejects it (`C2016`), and
an initializer on it then reports `C2078`. The emitted C must compile on
every ship and dev target, so an emitted type with no member is a defect
wherever it is produced.

**This class has two recorded instances.** The first was
`typedef struct Sub_N_EngWorld {}` for a zero-field opaque handle, closed
by giving it `char subscript_opaque;`. The second was the shadow-root
frame, whose declaration read a module fact and whose members read
function facts. Both are recorded in
`specs/tracking/windows-portability.md`.

CLAUDE.md's two-round rule applies at a second instance: a fix that
closes named sites does not converge. So the emitter carries a **total
check over its own output**, not a rule at each producer.
`emit_lir_c` scans the finished translation unit and fails with **every**
empty `struct`, `union`, or `enum` body it finds, each with its line and
its declared name. The check reads the emitted text, and the C standard
supplies the rule, so it compares two facts derived apart (core principle
9). A new producer of an empty type meets a build failure naming its
site, and no future round needs to find the site by hand.

The check is not a formatter and not a C parser. It skips comments and
string and character literals, so a brace inside either is not a member.

## 11e. A label is followed by a statement

*(2026-08-28, review of §68 consumers, M4.)* The emitted C placed a
declaration directly after a resume label: `resume_b6: SubFn t0 =
frame->b6_v14;`. C11 6.8.1 gives a label a statement, and a
declaration is not one; clang reports it under `-pedantic` as a C23
extension, and MSVC rejects it *(docs)*. The emitter writes `;` after
every label it emits, so a declaration that follows is a statement's
successor. `verify_no_empty_aggregate`'s neighbour checks the emitted
text for a label followed by a declaration, over every corpus entry.## 12. P5 C-header binding vertical slice

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

*(2026-08-28.)* This section read "Implemented and verified" for a month
while Win64 and SysV were absent. `5807d7b` (§68 step 2) replaced the HIR
consumer with the LIR transcriber and carried AAPCS64 across alone, so
every x86-64 dev host met the loud error above. Only the arm64 reference
machine ran, so no gate reported it
(`specs/tracking/windows-portability.md`). The guard that should have
reported it, `boundary_struct_by_value_supported`, was a `#[cfg(test)]`
predicate lowering never called — core principle 9's shape. The three
ABIs are one function, `plan_aggregate_arg(abi, leaves, size)`; lowering
and the test both read `AggregateAbi::of(triple)`, and the test pins the
ABI identity per triple, not a bool.

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
harness row removed — Q34 makes its pinned construct legal). *(§64,
2026-08-23: a generic async function with explicit type arguments
is also awaitable.)*

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

### 33.4 The escape rule was written and withdrawn

*(Owner, 2026-08-28: written as a rule, then withdrawn the same day on
measurement.)*

**What the rule said.** A value whose type transitively holds a
nullable boundary-aggregate field may not escape the activation that
built it, rejected as S015. It was written after a round built two
storage classes for the defect below and a fresh review rejected each
one on an escaping shape.

**Why it is withdrawn.** Escape is not the discriminator. Measured at
`8b23e3c`, on the shape the consumer reports — an inner boundary
aggregate holding a string and two arrays, built in a conditional
expression, stored in the outer value's nullable field:

    returned from the function     selectors 3, 4, 5 read 0
    built and used in one function selectors 3, 4, 5 read 0

The two are identical. The rule forbids the escaping half of a defect
that fails the same way without escaping, so it fixes nothing.

**And it rejects a program that works.** `corpus/accept/a125` returns
a value with a nullable boundary-aggregate field from three functions.
Measured at the same pin, its value survives a 200-frame recursive
call after the return:

    no-clobber=1:12:34
    clobber=407628
    after-clobber=1:12:34

S015 rejected `a125` at four sites. The code is removed from the table
and no diagnostic ships.

**Where the escape framing came from.** The round that raised it wrote
its corpus entry with a direct return and an array return. Both
candidate storage classes then failed on those forms, and the round
reported that the language needed a storage-ownership rule. The
consumer never escapes: it builds a descriptor and passes it to a
foreign call.

**What is decided.** Nothing new. §33.1's call-duration scratch stands.

**Escape was measured wrongly, and the rule is reinstated.**
*(2026-08-28, after the Fable phase review of the post-§70 arc,
findings C1 and C2. This replaces a paragraph that read "Escape is
measured, and it works".)*

The withdrawal above rested on two measurements: `a125` survives a
200-frame recursive call after returning such a value, and a probe
that returned `a159`'s outer value printed the right 23 lines. **Both
are reads of a dead frame that happened to return the expected
bytes.** The storage behind a value-class-to-nullable address is a C
automatic in the emitting function's frame (`cemit.rs`, `(void*)&(operand)`;
`lower/func.rs`, a stack slot). Nothing in the contract gives it a
lifetime past that activation, and a measurement that reads a dead
frame does not discriminate.

The measurement that does discriminate, at `2a65724`: `setup()`
stores a holder whose descriptor carries a conditional fragment
temporary into a module global and returns; `main` reads it.

    dev tier    program terminated abnormally (signal 11)
    ship tier   global-before-clobber=0:0:0:0:8015:8016
                expected <sum>:12:1:1:31:47

The tiers disagree and neither is the program's meaning.

**Rule, reinstated as S015.** A value whose type transitively holds a
nullable boundary-aggregate field may not escape the activation that
built it. The escape sites are S009's: a `return`, an assignment to a
module-level binding, a store into an array element, a store into a
reference-class field, a capture by a lambda. The check is local to
one function; a value passed down is a copy, and the callee's own
check covers what the callee does with it. The traversal is §33's
reachable set; do not write a second one.

**`a125` is in this class and passes by luck.** Its three
`boundaryVia*` functions return a target whose `blend` names a
temporary in the callee's frame. The entry's purpose is conditional-arm
narrowing, which does not need the return: each function consumes the
target where it builds it. The rewrite keeps every printed line, so
`a125.expected` does not move. If a line must move, the round stops
and reports it.

The record of this section, in order: a rule written from an
unverified diagnosis; withdrawn on a measurement that did not
discriminate; reinstated on one that does. The middle step is the
defect, and it is CLAUDE.md's rule that a claim about behaviour
requires running the system in the shape that can fail.

Corpus: `r160` (the `setup()`/`main` shape above; `tsc` accepts).
`a125` rewritten. `a159` unchanged: it does not escape.

#### The defect itself

A value-class-to-nullable conversion takes the address of the source
aggregate's storage. `codegen/src/root_storage.rs` typed an
`l::ValueType::Address(_)` as zero root slots, so no address kept its
base alive. Before `74a091c` every managed value stayed a root for the
whole activation and the address survived by accident. `74a091c` made
the storage scope the live range (§68.2 rule 8), and the base then died
while an address into it was live.

**This is a defect inside §68's form.** LIR carries address
provenance, and root storage must read it: a base stays rooted while an
address derived from it is live. One shared plan serves both tiers, so
the fix has no per-transcriber site.

### 33.5 The script-side representation is a managed box

*(Owner, 2026-08-29: "1,2,3 やりましょう", item 1, then "go on" on the
box design after the recursion measurement.)*

§33.1 gives a non-null struct-pointer member a pointer to a scratch
struct at the call. It said nothing about how the script holds the
member between construction and the call, and the tree held it as an
**address into activation storage**. That representation is the root
of §33.4's whole record, of §68.2 rule 8b, and of the S015 escape rule:
each exists to keep an address alive, or to forbid the program shapes
where it cannot be kept alive.

**Measured at `857757a`:** the mirror declares a recursive member,
`SubChainHeader { next: SubChainHeader | null }`, the intrusive
extension chain of §12.3. An inline representation (payload plus a
presence flag) cannot hold a type that contains itself, so the box is
the one representation, not one of two.

**Rule.** A member, element, or local of type `T | null`, where `T` is
a boundary value class, holds either `null` or a **managed reference to
a heap copy of `T`** — a box. The box is a Context allocation with the
allocation header every managed object carries; it is freed by
`Context.collect()` or with the Context, as an array or a string is
(invariant 2: a program that never collects is correct, merely
larger).

1. **A store copies.** Storing a `T` value into such a place
   allocates a fresh box and copies `T` into it (C12 value semantics;
   no two places alias one box through a value store). Storing `null`
   stores `null`. Storing a `T | null` value copies the reference,
   as a reference-class handle copies.
2. **A read through the narrowed member is a place in the box.**
   After `x.f !== null`, `x.f.a` reads and `x.f.a = v` writes the
   box's storage, as a field of a reference class does. `const c: T =
   x.f` copies out.
3. **The foreign call reads the box.** §33.1's scratch construction
   takes the box's contents for a non-null member and `NULL` for
   `null`; nothing else changes at the call.
4. **No address of activation storage is taken.** The
   value-class-to-nullable conversion is an allocation and a copy,
   not an address. §68.7.2's boundary-address `Coerce` row goes; the
   form gains one instruction that takes a value-class datum and
   produces the box's handle (the round names it inside §68's rules,
   the verifier checks it, the interpreter implements it from the
   row).
5. **Recursion is representable.** A box holds a `T` that holds a
   box. The read direction stays fail-loud per §33.1.
6. **S015 is deleted**, and its code-table row with it. The escape it
   forbade is a copy of a reference, and it is sound. `r160` is
   deleted and its program becomes accept entry `a169`, printing every
   selector through the foreign checker after the escape.
7. **Rule 8b stays** for `AddressOfValue`, its remaining client (a
   by-value receiver). Its `Coerce` client is gone.
8. **What does not move.** `a106`, `a125` (as restructured), `a159`,
   `a163`, and the two fixtures restructured under S015 keep their
   goldens: an in-activation shape is unchanged by where the payload
   lives. `a125`'s original returning form is legal again, and a
   later round can restore it if the owner wants the entry in its
   first shape.

**Cost.** One allocation per non-null store, where the address
representation had none. The count of such stores in a program is the
count of descriptors it builds, which is small and not in any hot
loop the corpus measures; `perf-gate`'s two workloads build none.
`a163` gains a `live_bytes` line so the boxes are counted.

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
position (r105, S013, `tsc`-clean). *(§64, 2026-08-23: the generic-class
rejection is withdrawn; r104 retires.)* Async methods on `@Descriptor`
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

## 56. R26 — integer literals read at the target's width

A downstream report (R26, 2026-08-11): a `u64` literal above
9007199254740991 (2^53 − 1) fails with S008, and the value is a
valid `u64`. WebGPU types buffer sizes, offsets, and copy sizes as
`u64`. A downstream test computes such a constant instead of
writing it.

Measured here at `d641d6d`: these three literals all fail with
S008.

```
const a: u64 = 9223372036854775807;
const a: u64 = 0xFFFFFFFFFFFFFFFF;
const b: i64 = -9223372036854775808;
```

Stock `tsc` accepts all three shapes (measured: exit 0). TS 80008
is a suggestion, not an error, so invariant 5 holds for the full
64-bit ranges.

The cap is the C3 decision: "no surface spelling above 2^53 − 1;
revisit with evidence" (`collisions.md` §3). R26 is that evidence.
The cause: `check_num_lit` (`compiler/src/check/expr.rs`) reads the
parser's `f64` view of the literal and range-checks through `f64`.
The spelling (`raw`) is available and exact.

### 56.1 Rule

In an integer context of type `T`, the checker reads the literal
from its spelling, not from the `f64` view. The checker accepts the
literal exactly when its mathematical value is in `T`'s range:

- `u64`: 0 to 18446744073709551615.
- `i64`: −9223372036854775808 to 9223372036854775807.
- Narrower types: unchanged ranges.

The reader handles every spelling the parser accepts: decimal,
`0x`/`0X`, `0b`/`0B`, `0o`/`0O`, and `_` separators. A leading
minus reaches the checker as the C4 fold. The reader applies the
sign before the range check, so `-9223372036854775808` is one
`i64` literal. If a literal has no spelling (a synthesized node),
its numeric value is exact and the present path stands.

Unchanged: a fractional or exponent spelling in an integer context
is an error; a float context keeps the `f64` reading; a
context-free integer literal defaults to `i32`; the S008 message
keeps its form.

### 56.2 The bit pattern in the HIR

`ExprKind::Int(i64)` stores the two's-complement bit pattern, and
the expression type names the interpretation. Each consumer reads
the bits at the expression's type:

- Cranelift `iconst` takes the bits. Correct today; no change.
- The C emitter's `int_literal` reinterprets by type
  (`v as u64` with the `ull` suffix). Correct today for `u64`. For
  `i64` it prints `{v}ll`, and that spelling is invalid C for
  `i64::MIN`. It must print `(-9223372036854775807ll - 1)`, the
  treatment the function gives `i32::MIN` today.
- The literal shift-amount check compares the bits as `i64`
  (`check/expr.rs`). A `u64` amount with a negative bit pattern
  passes it. The check reads the amount at the operand's type.

### 56.3 The ambient mirror channel

`int_literal_value` (`compiler/src/check/mod.rs`) feeds mirror flag
constants (`declare const X = <int literal>;`, §13.2) and also
reads through `f64`. It reads the spelling at `u64` width instead.
`bindgen` then drops its fail-loud guard for flag values above
2^53 − 1 (`bindgen/src/emit.rs`). That guard cited the C3 cap
(`p5-interop.md` MINOR m2), and this section removes the cap. The
bindgen unit test for the guard becomes an acceptance test.

### 56.4 What does not change

- The runtime and both tiers carry exact 64-bit integers today.
  The downstream measured it: `base * 2048` prints
  18446744073709549568. This change reaches the literal channel
  only.
- Every pre-existing golden stays byte-identical.
- `as` conversions, the mixed-arithmetic rules, and C4 contextual
  typing stand.

### 56.5 Exit criteria (pre-registered)

1. Record the red first: the new accept entry fails with S008 at
   `d641d6d`.
2. New accept entry `a132-int-literal-64bit`: the `u64` maximum in
   decimal, the same value in `0x` and in `_`-separator form,
   9007199254740993 (2^53 + 1), the `i64` maximum, and the `i64`
   minimum; the program prints each one. The entry passes the
   checker gate, the `tsc` gate, and the tier-differential gate
   with a committed golden.
3. New reject entries with S008, registered in `corpus_reject.rs`:
   `r124-u64-literal-overflow` (18446744073709551616) and
   `r125-i64-literal-underflow` (-9223372036854775809).
4. Unit tests: the checker stores the bit pattern for the `u64`
   maximum and the `i64` minimum; the shift check rejects a `u64`
   amount of `0xFFFFFFFFFFFFFFFF`; the emitted C for an `i64::MIN`
   literal compiles and runs; a mirror flag constant above 2^53
   reaches a program with the exact value; `bindgen` emits a flag
   above 2^53.
5. The workspace gate passes in both profiles; every pre-existing
   golden stays byte-identical; `cargo fmt --check` and the `tsc`
   gate exit 0.

## 57. R27 — field initializers run on every construction

A downstream report (R27, 2026-08-15): a `@CStruct` class with no
constructor and a field initializer `value: i32 = 37` prints
`field:37` on the dev tier and `field:0` on the ship tier. Both
tiers compile the program clean, and no trap fires. The downstream
avoids the shape with a generator rule and escalates the silent
divergence.

Measured here at `b1a5dab`, with `run_jit` / `run_c_aot`:

1. Value class, no constructor, `value: i32 = 37`:
   dev `field:37`, ship `field:0`. The downstream shape.
2. Reference class, no constructor, `value: i32 = 41`:
   dev `field:41`, ship `field:0`.
3. Constructor present, an initializer that calls a print helper,
   an argument that calls another: dev prints `init runs` before
   `arg runs`; ship prints `arg runs` before `init runs`.
4. `this` in a field initializer: the checker accepts it; the dev
   tier fails with `internal lowering error: `this` outside a
   method`, with or without a constructor; the ship tier runs it.

Cause, by site:

- The C emitter runs field initializers only inside the emitted
  constructor (`emit_constructor`, `codegen/src/cemit.rs`). For a
  class with no constructor, value `new` lowers to a zero literal
  and reference `new` to a bare allocation. Findings (1), (2).
- The Cranelift lowering (`eval_new`,
  `codegen/src/lower/func.rs`) runs the initializers at the
  construction site, before it evaluates the constructor
  arguments. The C emitter evaluates the arguments first.
  Finding (3).
- The checker (`check_class_body`, `compiler/src/check/mod.rs`)
  checks a field initializer in a context with a `this` binding.
  Neither tier defines that lowering. Finding (4).

Under `node`, the same source prints `arg runs` before
`init runs`, and a constructor-less class carries the initialized
value (measured, exit 0). The ship tier's with-constructor order
is the TS order; the dev tier's no-constructor result is the TS
result. Each tier is correct where the other is wrong.

### 57.1 Rule

For `new C(...)` of a non-boundary, non-descriptor class, both
tiers observe this order:

1. The argument expressions evaluate left to right.
2. The construction zero-initializes the instance.
3. The declared field initializers run in declaration order, once
   per construction, with or without a declared constructor.
4. The declared constructor body runs after the initializers.

Steps 2–4 are ordered against step 1 only where observable: no
argument side effect interleaves with an initializer or with the
constructor body.

A field initializer must not read `this`. The checker checks each
field initializer in a context with no `this` binding, so `this`
there gets the standing S100 diagnostic "`this` is only available
in constructors and methods". No program that uses it runs on the
dev tier today (finding 4), so nothing regresses.

### 57.2 Changes by site

- C emitter, class with no constructor and one or more
  initializers: value `new` materializes a zeroed temporary, runs
  the initializers into it, and yields it; reference `new` stores
  each initializer after the allocation. The with-constructor path
  stands — it already runs arguments, then initializers, then the
  body.
- Cranelift `eval_new`: the constructor arguments evaluate before
  the field initializers run. The no-constructor path stands.
- Checker `check_class_body`: the field-initializer context
  carries no `this` type.

Out of scope, unchanged: boundary-struct positional `new` (an
ambient mirror declares no field initializer) and Q33 descriptor
defaults (§25, §43).

### 57.3 What does not change

- Every pre-existing golden stays byte-identical. The green
  differential gate at `b1a5dab` proves no committed entry
  observes the missing initializers or the order.
- A with-constructor program whose initializers have no side
  effects keeps its bytes on both tiers.

### 57.4 Exit criteria (pre-registered)

1. Record the red first, at `b1a5dab`: the two new accept entries
   each produce different bytes on the two tiers; the new reject
   entry passes the checker.
2. New accept entry `a133-field-init-no-ctor`: a `@CStruct` value
   class and a reference class, each with no constructor and a
   non-zero field initializer; `main` prints the fields. The entry
   passes the checker gate, the `tsc` gate, and the
   tier-differential gate with a committed golden.
3. New accept entry `a134-field-init-order`: a class with a
   constructor, an initializer that calls a print helper, and a
   constructor argument that calls another. The golden pins
   `arg runs` before `init runs`.
4. New reject entry `r126-this-in-field-init`: `this` in a field
   initializer, S100, registered in `corpus_reject.rs`.
5. Unit tests: the emitted C for a constructor-less initialized
   value class contains the initializer store, and the same for a
   reference class; the checker rejects `this` in a field
   initializer with S100.
6. The workspace gate passes in both profiles; every pre-existing
   golden stays byte-identical; `cargo fmt --check` and the `tsc`
   gate exit 0.

## 58. R29 — a class index signature is accessor sugar

Origin: downstream request R29, 2026-08-15, at pin `dfef090`. The
downstream authors GPU kernels in subscript and compiles the typed
HIR to WGSL. A kernel body is index-dense: `out[i] = f(a[i], b[i])`
is the common case. The checker rejects `a[i]` on the downstream's
generic wrapper classes, so their authors must write `get`/`set`
calls.

Measurements at the pin, on this host:

1. The report reproduces: `a[i]` on a generic class fails with
   S100 "type `...` is not indexable". `check_index`
   (`compiler/src/check/expr.rs`) indexes `Type::Array` and
   `Type::FixedArray` alone.
2. The handoff sketches ambient wrappers
   (`declare class StorageArray<T> { ... }`). That form does not
   check at the pin through any route. Mirror ingestion rejects a
   generic `declare class` at the declaration ("mirror declaration
   form outside the decided surface", `collect_mirror_decl`,
   `compiler/src/check/mod.rs`). A non-mirror `declare class`
   rejects each body-less method ("function bodies are required").
   An arrow-typed field is not callable as a method.
3. The form that checks at the pin is the generic script class
   with method bodies. Its methods check, and an `unreachable()`
   body satisfies a generic return (measured, exit 0). The
   handoff's own error message comes from this form.
4. Stock `tsc` accepts an index signature on a class with a
   sized-alias index type — `readonly [index: u32]: T` and the
   mutable form — and accepts reads and writes through it beside
   named members (measured, exit 0).

Decision: the language reads index signatures on class
declarations and defines them as accessor sugar. The rewrite is
complete in the checker, so the tiers agree by construction and
the downstream's HIR consumer sees the method calls it already
handles.

### 58.1 Rule

1. A class declaration can declare at most one index signature:
   `[index: I]: T` or `readonly [index: I]: T`. `I` is `i32` or
   `u32`.
2. Reference classes only. An index signature on a value class
   (`@CStruct`) or a descriptor class fails with S100.
3. The class must declare a method `get(index: I): T`. If the
   signature is not `readonly`, the class must also declare
   `set(index: I, value: T): void`. The types must match the
   signature exactly. A missing or mismatched accessor fails with
   S100 at the signature.
4. A read `a[i]` checks to the same HIR as `a.get(i)`. A write
   `a[i] = v` in statement position checks to the same HIR as
   `a.set(i, v)`. No new HIR form exists, and no tier changes.
5. The index expression checks against `I` by ordinary
   assignability (S007 on a numeric mismatch).
6. If the signature is `readonly`, the write spelling fails with
   S100 at the assignment.
7. Rejected spellings, each with S100 and a message that names the
   spelling: compound assignment `a[i] op= v`, increment and
   decrement on `a[i]`, and the write used as a value.
8. The language adds no bounds rule. The accessor body owns the
   behavior at every index value.
9. Nothing else moves. Array and `FixedArray` indexing keep the
   `i32` index rule. Mirror ingestion keeps rejecting generic
   classes and does not read index signatures.

`collisions.md` gains C10: JS reads a numeric property through an
index signature; subscript calls the declared accessor.

### 58.2 Changes by site

- `compiler/src/check/expr.rs` (`check_index`): a `Type::Class`
  receiver whose class declares an index signature rewrites the
  read to the `get` call. The blanket `i32` index rule stays for
  arrays.
- `compiler/src/check/expr.rs` (`check_assign` and the update
  path): an index target on a signature class rewrites the plain
  write to the `set` call, and rejects the `readonly`, compound,
  increment, and value-position spellings.
- Class collection (`compiler/src/check/mod.rs`, class HIR): parse
  the index-signature member, store index type, element type, and
  the `readonly` flag, and validate the accessors at the
  declaration.
- Mirror ingestion: unchanged. A unit test pins that an
  index-signature member in a mirror class keeps failing.
- No codegen, runtime, or prelude change.

### 58.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: `a136` fails at the signature
member and at the index spellings; `r128`–`r130` fail for a
different reason than their registered code. Record the
diagnostics before the implementation.

1. `corpus/accept/a136-index-signature.ts` + `.expected` (golden
   from the dev JIT; ship byte-identical): a generic read-only
   wrapper and a generic mutable wrapper over a `data: T[]` field;
   writes through `m[i] = v`; reads through `m[i]`; one line pins
   `m[0] === m.get(0)` as `true`; two element types pin
   monomorphization.
2. `corpus/reject/r128-readonly-index-write.ts`: S100 at the
   write.
3. `corpus/reject/r129-index-signature-no-get.ts`: S100 at the
   declaration.
4. `corpus/reject/r130-index-compound-assign.ts`: S100 at the
   compound write.
5. Unit tests in the same commit: a wrong-typed index argument
   fails with S007; a value-class signature fails; a
   value-position write fails; a mirror index-signature member
   fails; the checker produces identical HIR for the sugar and the
   spelled calls.
6. Counts: accept single files 134 → 135; accept source files
   136 → 137; rejects 123 → 126; goldens 135 → 136. The generated
   docs regenerate byte-exact.
7. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.

## 59. R30 — host-called entries take handle and scalar parameters

Origin: downstream request R30, 2026-08-16, at pin `e8e01d9`. The
engine-embedded class moves from pull (`engineAcquireDevice` inside
the entry) to push: the host passes long-lived handles into
entries. The downstream asks for the pin; the borrow discipline
above the language stays theirs.

Measurements at the pin, on this host:

1. The checker accepts handle-typed and reference-class-typed
   parameters on exported functions (probe, exit 0).
2. The ship tier emits `subscript_export_<name>` only for a
   zero-argument `void` export (`emit_exports`,
   `codegen/src/cemit.rs`). A parameterized export compiles to a
   `static` function with no host symbol. `a23`'s
   `update(dtFixed: f32)` runs only because `main` calls it
   in-script.
3. The dev session's host surface,
   `ReloadSession::call_export`, resolves only zero-argument
   `void` entries (`codegen/src/reload.rs`).
4. The AOT host hooks receive `subscript_rt_context*`
   (`codegen/src/aot.rs`), so a fixture hook can call an export
   wrapper directly. No new ship-harness machinery is required.

### 59.1 Rule

1. An exported function is **host-callable** when it is
   synchronous, returns `void`, and every parameter is a boundary
   scalar (sized numeric, `boolean`) or an opaque handle.
   Zero-argument `void` async exports stay host-callable,
   unchanged.
2. For every host-callable export, the ship tier emits
   `void subscript_export_<name>(subscript_rt_context* ctx, ...)`
   with the same parameter C types as the internal function.
3. The dev session gains
   `call_export_with(name, &[EntryArg]) -> Result<(), RunError>`.
   `EntryArg` covers opaque handles and the boundary scalars. An
   unknown name, an arity mismatch, or an argument-kind mismatch
   fails with `RunError::Internal`, and no script code runs.
   `call_export` keeps its zero-argument behavior.
4. An exported function that is not host-callable stays a legal
   script export with no host symbol. The checker changes nothing
   and rejects nothing new, so this cycle has no reject-corpus
   entry. The convention comments in the emitted C name the
   host-callable subset.
5. At the C level, a parameter is a borrow for the duration of
   the call. Handle values are copyable; the script can wrap and
   store them, and the borrow discipline above the language is
   the host's.
6. Parameter marshaling equals the foreign-call marshaling for
   the same types.

### 59.2 Changes by site

- `codegen/src/cemit.rs` (`emit_exports`): emit the parameterized
  wrappers.
- The host-export convention text lives in
  `runtime/src/host_header.rs` and reaches the emitted C through
  the generated `runtime/include/subscript_runtime.h` and
  `AOT_ENTRY_C` (`codegen/src/aot.rs`). Update the generator;
  regenerate the header. *(Correction: the contract first
  attributed this text to `cemit.rs`; the implementer measured
  the real sites.)*
- `codegen/src/reload.rs`: the entries table records each
  host-callable export's parameter signature; add `EntryArg` and
  `call_export_with`. `codegen/src/lib.rs` re-exports `EntryArg`.
- The JIT generation site that fills the entries table records
  the signatures.
- `codegen/src/bin/capture.rs` and `codegen/tests/golden.rs`:
  drive `a137` on the a128 pattern — dev through
  `call_export_with` then `call_main`; ship through a pre-entry
  hook that calls the export wrapper.
- `corpus/interop/interop.c`: a hook
  `subHostOwnedStateAdoptDrive(subscript_rt_context*)` borrows
  the host state, advances it once, and calls
  `subscript_export_adopt(ctx, state, 7)`. The hook lives in
  `interop.c` only — never in `interop.h`; the mirror must not
  move.
- `codegen/tests/support/native_fixture.rs`: bindings so the dev
  path obtains the same handle and advances it once before
  `call_export_with`.
- No checker, runtime, or prelude change.

### 59.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the emitted C for a parameterized
export contains no `subscript_export_` wrapper for it (record the
grep), and the dev session has no argument-taking call. Record
both before the implementation.

1. `corpus/accept/a137-handle-entry-param.ts` + `.expected`:
   `export function adopt(state: SubHostOwnedState, tag: i32)`
   wraps the handle in a reference class, stores it in a module
   global, and prints the tag; `main` advances twice through the
   stored wrapper and prints the counts. The host side advances
   once before it calls `adopt`, so the printed sequence proves
   the same host object crossed the parameter. Golden from the
   dev JIT; ship byte-identical.
2. Unit tests in the same commit: the emitted C for a probe
   program contains the parameterized wrapper with handle and
   scalar C types; a parameterized async export gets no wrapper;
   `call_export_with` fails on wrong arity and on a wrong
   argument kind; `call_export` still runs a zero-argument entry.
3. Counts: accept single files 135 → 136; accept source files
   137 → 138; goldens 136 → 137; rejects unchanged at 126. The
   generated docs regenerate byte-exact.
4. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical; the
   interop mirror regenerates byte-identically.

## 60. R31 — `using` declarations: deterministic scope-exit dispose

Origin: downstream request R31, 2026-08-16, at pin `e8e01d9`
(their pin `dae6e10`). Their script-driven programs hold 577
`dispose()` call sites across two suites; `a27-host-compute.ts`
alone ends in a 16-line reverse-order release tail with two
early-return failure paths, and every program that reads a GPU
result crosses at least one `await` between creation and disposal.
Three owner decisions (2026-08-16) pre-date this contract and are
its starting point: nullable `using` is rejected; a trap does not
run dispose; the cleanup member is `dispose`, with
`[Symbol.dispose]` as the hook.

Measurements at the pin, on this host:

1. The pinned `swc` parses `using` declarations and
   `[Symbol.dispose]()` class members. Both reach the checker and
   fail with S100 ("nested declarations are not in the decided
   surface"; "computed method names are not decided"). The parser
   needs no change.
2. `tsc` 5.9.2 with `lib: ["ES2022"]` fails the hook member with
   TS2318/TS2550; with `lib: ["ES2022", "ESNext.Disposable"]` the
   same program exits 0.
3. Under `node` v24.18.0 (exit 0): bindings dispose at block end
   in reverse declaration order; a return expression evaluates
   before the disposals; an early return disposes only the
   bindings in scope at that point; a loop disposes per iteration,
   and `break` disposes the current iteration's bindings; an
   `async` function that suspends across `await` disposes at
   completion, after the resume.

### 60.1 Rule

1. A reference class can declare the member
   `[Symbol.dispose](): void` — non-static, non-async, no
   parameters, `void` return. The member is a method under a
   reserved internal name that no source identifier can spell. A
   value class or a descriptor class that declares it fails with
   S100. Every other computed member name stays
   rejected, and the explicit call spelling `x[Symbol.dispose]()`
   stays rejected — a class that wants a manual spelling declares
   an ordinary method (the downstream uses `dispose()`).
2. `using x = expr` declares an immutable binding in a function
   block scope. A module-level `using` and a `using` in a `for`
   head fail with S100. A `using` statement can declare several
   bindings; their disposal order is the reverse binding order.
3. The initializer's static type must be a reference class that
   declares the hook. Any other type — nullable types included —
   fails with S100. Narrow first, then bind (owner decision).
4. Dispose runs at every exit of the owning scope: the natural
   end, `return`, `break`, `continue` that leaves the scope, and
   the end of each loop iteration. Order: reverse declaration
   order within a scope; the innermost scope first when one exit
   leaves several scopes. A `return` expression evaluates into a
   synthesized local before the disposals run. (All measured
   under `node`, §60 item 3.)
5. A coroutine suspension is not a scope exit. A frame that
   suspended across the binding disposes at completion (measured
   under `node`).
6. A trap does not run dispose (owner decision; §18.1b, no
   rollback).
7. `await using` fails with S100. Every downstream disposal is
   synchronous.
8. The rewrite is checker-complete. After checking, a `using`
   declaration is a `const` binding plus hook-method calls
   inserted at the scope exits; the HIR contains only forms that
   exist today. No new HIR node, no codegen, runtime, or prelude
   change; the tiers agree by construction.
9. The language adds no aliasing or use-after-dispose rule. The
   hook is an ordinary method call; the class owns its behavior
   after disposal.
10. `tsconfig.json` `lib` gains `"ESNext.Disposable"`. No other
    gate changes.

`collisions.md` gains C11: JS skips a null `using` binding and
runs disposal during throw-unwind; subscript rejects the nullable
binding at check time and does not dispose on a trap.

### 60.2 Changes by site

- `compiler/src/check/mod.rs`, `compiler/src/hir.rs`: accept the
  hook member on reference classes under the reserved internal
  method name; validate its shape at the declaration.
- The statement checker: check `using` declarations, track live
  bindings per scope, and insert the hook calls at every scope
  exit, including the synthesized return local.
- `tsconfig.json`: the `lib` line.
- No parser, codegen, runtime, or prelude change.

### 60.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: record the S100 diagnostics for
the corpus entries below before the implementation.

1. `corpus/accept/a138-using-dispose.ts` + `.expected` (golden
   from the dev JIT; ship byte-identical): a reference class whose
   hook prints its label; one block with two bindings (reverse
   order); a helper with a return value (the value line prints
   before the dispose line); an early return; a loop with `break`
   (per-iteration disposal). The printed sequence must equal the
   `node` measurement shape from §60 item 3.
2. `corpus/accept/a139-using-async.ts` + `.expected`: an
   `export async function main` with a binding that crosses an
   `await`; the golden pins the resume line before the dispose
   line.
3. `corpus/reject/r131-using-nullable-init.ts`: a nullable
   initializer, S100. `corpus/reject/r132-await-using.ts`:
   `await using`, S100.
   `corpus/reject/r133-using-without-dispose.ts`: an initializer
   type without the hook, S100.
4. Unit tests in the same commit: module-level `using` fails; a
   `for`-head `using` fails; a value-class hook member fails; a
   non-hook computed member name still fails; the explicit
   `x[Symbol.dispose]()` spelling still fails; a multi-binding
   `using` statement disposes in reverse order.
5. Counts: accept single files 136 → 138; accept source files
   138 → 140; rejects 126 → 129; goldens 137 → 139. The generated
   docs regenerate byte-exact.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate with
   the new `lib` entry; every pre-existing golden and `.expected`
   byte-identical.

## 61. R32 — a wire-mapped alias in an entry signature

Origin: downstream request R32, 2026-08-16, at pin `1f875da`. The
downstream's first R30 consumer declared
`init(instance, device, format: GPUTextureFormat)` and the checker
rejected the export. R30's response invited the widening with
evidence; this is that evidence. The downstream does not ask for
plain string-literal unions in entry signatures — those have no C
representation, and the rejection stays right for them.

Measurements at the pin, on this host:

1. The report reproduces: a `CEnum` alias parameter on an exported
   function fails with S100 "exported function `init` has a
   string-literal union alias in its boundary signature". The
   rejection site checks every export for any string alias in
   params or return (`compiler/src/check/mod.rs`,
   `contains_string_alias`).
2. The wire channel exists in both directions at the bind
   boundary (R23/R24, §52): the ship tier validates through
   `validate_wire_alias` and `subscript_rt_trap_wire_enum`
   (`codegen/src/cemit.rs`); the dev tier calls the same trap
   symbol (`codegen/src/lower/func.rs`). Unknown wire values trap
   with the alias name (`corpus/trap/t48`, `t49`).

### 61.1 Rule

1. An exported function **parameter** can be a wire-mapped
   (`CEnum`) string-alias type. The checker keeps rejecting a
   plain (unwired) string alias in any exported signature, and
   any string alias in an exported **return** type.
2. R30's host-callable subset (§59.1) widens: every parameter is
   a boundary scalar, an opaque handle, or a wire-mapped alias.
3. At the host ABI, the parameter is the wire value as `int32_t`:
   the ship wrapper takes `int32_t`, and the dev tier accepts
   `EntryArg::I32`.
4. The crossing validates exactly as a bound-function return
   does (R23): a wire value outside the alias's table traps with
   the alias name and the parameter's position, **before the
   entry body runs**. A mapped value enters as the alias value.
5. In-script calls to the same exported function do not change:
   they never pass through the host wrapper, so no wire
   validation runs on the script-internal path.

### 61.2 Changes by site

- `compiler/src/check/mod.rs`: the export-signature check permits
  a wire-mapped alias in a parameter and keeps both remaining
  rejections.
- `codegen/src/lower/mod.rs`: `is_host_callable_export` and
  `entry_param_kind` admit the wire-mapped alias (kind `I32` at
  the ABI); the reload adapter validates the wire value and calls
  `subscript_rt_trap_wire_enum` on a miss, then returns without
  the entry call.
- `codegen/src/cemit.rs` (`emit_exports`): for a wire-alias
  parameter the wrapper body validates before the internal call,
  with a `pos_id` at the parameter declaration.
- `corpus/interop/wire-enum.c`: a drive hook on the a137 pattern
  (in the `.c` only; the header and the mirror must not move),
  with the same weak-fallback linking device.
- Test harnesses (`codegen/tests/golden.rs`, the trap-corpus
  support, `codegen/src/bin/capture.rs`): drive the new accept
  and trap entries on both tiers.

### 61.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 above, recorded (this
host, probe with the wire-enum mirrors, exit 1).

1. `corpus/accept/a140-wire-entry-param.ts` + `.expected`: an
   entry takes a `SubWireMode` parameter and a scalar; the host
   passes wire `23` (`"m1"`); the script prints the alias value
   and the scalar. Golden from the dev JIT; ship byte-identical.
2. `corpus/trap/t50-wire-entry-unknown-value.ts` + `.expected`:
   the host passes wire `12345`; the trap carries the alias name
   and the value, and the entry body never prints.
3. `corpus/reject/r134-plain-alias-entry-param.ts`: a plain
   string-literal union (no wire table) in an entry signature
   keeps the S100.
4. Unit tests in the same commit: the emitted wrapper for a
   wire-alias entry contains the validation and the trap call; an
   `EntryArg::I32` outside the table traps on the dev tier before
   the entry body runs; a wire-alias return type on an export
   still fails.
5. Counts: accept single files 138 → 139; accept source files
   140 → 141; goldens 139 → 140; rejects 129 → 130; trap corpus
   49 → 50. The generated docs regenerate byte-exact.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical; the
   wire-enum mirror regenerates byte-identically.

## 62. R33 — an alignment override on `@CStruct` value classes

Origin: downstream request R33, 2026-08-22, at pin `4313dcf`. The
downstream uploads `FixedArray<T, N>` of `@CStruct` classes to GPU
buffers with no encoder. That path is correct only when the C layout
equals the WGSL layout. WGSL aligns `vec3<f32>` and `vec4<f32>` to 16
and `vec2<f32>` to 8; C aligns `{ x: f32; y: f32; z: f32 }` to 4. No
spelling today raises the alignment of a value class. Owner decision
of 2026-08-22: the language gains a class-level override.

Measurements at the pin, on this host:

1. The call form `@CStruct({ align: 16 })` fails with S100 "the only
   decided decorators are the ambient `@CStruct` and `@Descriptor`"
   (`compiler/src/check/mod.rs`, `class_decorators`: it matches
   `Expr::Ident("CStruct")` only). The class is then not a value
   class, and a field of that type fails the value-class whitelist.
2. Two sites compute the class layout and both take the alignment as
   the maximum field alignment: `codegen/src/layout.rs`
   (`class_layout`, feeds `Repr::Agg` and `StructLayout`) and
   `compiler/src/check/layout.rs` (the aggregate-limit validator).
3. The ship tier emits `typedef struct {name} { ... }` with no
   alignment attribute (`codegen/src/cemit.rs`, `emit_one_typedef`).
   Both tiers compile C as C11 (`-std=c11`, `/std:c11`).
4. Apple clang 21, C11, with `_Alignas(16)` on the first field:
   `Vec3f` 16/16 at offsets 0,4,8; `Particle { pos, vel }` 32/16 at
   0,16; `Mixed { a: f32; p }` 32/16 at 0,16; `Mat3x3f` 48/16 at
   0,16,32; `Vec3f a[4]` stride 16; `Vec2f` with `_Alignas(8)` 8/8.
   These equal the downstream's table and the WGSL offsets.
5. Stock `tsc` 5.9.2 with the overload in 62.2 accepts
   `@CStruct({ align: 16 })` and rejects `align: 3` (TS2322), an
   unknown key (TS2353), and `@Descriptor({ align: 16 })` (TS2554).
6. Heap objects start at a 16-aligned payload (`HEADER_SIZE` 16,
   allocation alignment 16 on both the host allocator and the ship
   arena), so a 16-aligned field inside a reference class is aligned
   in memory on both tiers.

### 62.1 Rule

1. `@CStruct({ align: N })` declares a value class whose alignment is
   `N`. `N` is an integer literal in `{2, 4, 8, 16}`. The size is the
   natural size rounded up to `N`. Field offsets do not change.
2. `N` must be greater than or equal to the natural alignment. A
   smaller `N` is an S100 whose message names both numbers: the
   requested `N` and the natural alignment.
3. A class with the override is an ordinary value class everywhere
   else: a field of another value class or of a reference class, a
   `FixedArray` element, a local, a parameter, a module global. The
   containing aggregate and the `FixedArray` stride use the overridden
   size and alignment, as C does.
4. Both tiers carry the same numbers. The dev tier's `Repr::Agg
   { size, align }` and the ship tier's C struct agree, and the
   `offsetof` proof (§12.3, `codegen/tests/offsetof_layout.rs`)
   asserts `sizeof` and `_Alignof` for a class with the override.
5. The options literal accepts the key `align` only. Any other key, a
   non-literal value, a value outside the set, a second argument, an
   empty literal, and the call form on `@Descriptor` are each an S100.
6. A generic value-class template carries the override into every
   instantiation.
7. Copy-on-assign, field initializers (§57), constructors, and the
   value-class whitelist do not change. The override is layout only.
8. Out of scope: a per-field `align` or `size`, an alignment above 16,
   and an alignment imported from a C header through `subscript bind`.
   A mirror-ingested boundary struct never carries the override, so
   no overridden class crosses the host ABI by value.

### 62.2 Changes by site

- `prelude/lang.d.ts`: one overload beside the existing declaration,
  so stock `tsc` accepts the call form:

  ```ts
  declare function CStruct(options: { align: 2 | 4 | 8 | 16 }):
    <T extends abstract new (...args: never[]) => object>(
      target: T, context: ClassDecoratorContext) => T;
  ```

- `compiler/src/hir.rs` `ClassDef`: one optional field, the alignment
  override, `None` for every class without it.
- `compiler/src/check/mod.rs` `class_decorators`: accepts
  `Expr::Call` with callee `CStruct` and one object-literal argument;
  reads `align`; reports the 62.1 rule 5 rejections at the decorator
  span. `GenericClass` carries the override (rule 6). The rule 2 check
  runs where the class layout is known.
- `compiler/src/check/layout.rs` and `codegen/src/layout.rs`: the
  final alignment is `max(natural, override)`, and the size rounds up
  to it. `StructLayout.align` reports the overridden value.
- `codegen/src/cemit.rs` `emit_one_typedef`: when the class has the
  override, the first field declaration carries `_Alignas(N)`. C11
  gives the struct that alignment and rounds `sizeof` (measured, 62
  item 4, clang). MSVC `cl /std:c11` accepts `_Alignas` *(docs)*; the
  windows-msvc run confirms it.
- `codegen/tests/offsetof_layout.rs`: the proof adds classes with the
  override. The C side declares the same structs with `_Alignas` in
  the probe source; `corpus/interop/interop.h` does not change (rule
  8).
- `compiler/src/language_reference.rs`: one sentence on the override
  in the value-class entry; `generated-docs/` regenerates.

### 62.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 in 62 item 1, recorded
(this host, exit 1).

1. `corpus/accept/a141-cstruct-align.ts` + `.expected`: `Vec3f`
   with `align: 16` and three `f32` fields; `Mixed { a: f32; p: Vec3f
   }`; a `FixedArray<Vec3f, 4>` field; values copied on assignment
   and printed through field reads. Golden from the dev JIT; ship
   byte-identical.
2. `corpus/reject/r135-cstruct-align-below-natural.ts`: `align: 2`
   on a class whose natural alignment is 4; S100 with both numbers.
3. `corpus/reject/r136-cstruct-align-not-in-set.ts`: `align: 3`;
   S100.
4. `corpus/reject/r137-descriptor-align.ts`: `@Descriptor({ align:
   16 })`; S100.
5. Unit tests in the same commit: `value_class_layouts` reports
   (16, 16) for `Vec3f`, (32, 16) with `p` at 16 for `Mixed`, and
   (48, 16) for a three-`Vec3f` class; the emitted C typedef carries
   `_Alignas(16)` on the first field; the `offsetof` proof covers the
   override classes; an unknown key and a second argument are each
   S100.
6. Counts: accept `.ts` 139 → 140; `.expected` 140 → 141; rejects
   130 → 133. The generated docs regenerate.
7. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.

## 63. R35 — a discovery check for one unresolved import

Origin: downstream request R35, 2026-08-22, at pin `ba6aa2e`. The
downstream generates a support module `<stem>.typegpu.ts` from the
schemas in `<stem>.ts`, and the program imports that module. The
generator must read the program's HIR before the module exists.
Today the first `check_program` fails at the import and returns no
HIR, so the downstream scans the import statement with a second
parser, which its own rules forbid.

Measurements at the pin, on this host:

1. `resolve_imports` (`compiler/src/check/mod.rs`) reports S100
   "imported module `./x` is not among the program's files" and
   binds nothing; the check returns `Err`.
2. `Type::Error` is "assignable everywhere so one error does not
   cascade" and is never in a successful check's HIR
   (`compiler/src/types.rs`). `unknown name` (expressions) and
   `unknown type name` (types) are the two lookup-failure sites.
3. `parse_import_specifiers` (`compiler/src/parse.rs`) returns the
   module specifiers only, not the imported names.

### 63.1 Rule

1. New public API in `subscript_compiler`:

   ```rust
   #[non_exhaustive]
   #[derive(Debug, Clone, Default)]
   pub struct CheckOptions {
       /// Import specifiers to bind as poisoned when absent.
       pub poison_missing_modules: Vec<String>,
   }
   pub fn check_program_with(files: &[SourceFile], options: &CheckOptions)
       -> Result<hir::Module, Vec<Diagnostic>>;
   ```

   `check_program(files)` equals `check_program_with(files,
   &CheckOptions::default())`.
2. A specifier in the option and an import's source match after the
   normalization `resolve_imports` applies (strip a leading `./` and
   a trailing `.ts`). A listed module that is among the files
   resolves as today; the option has no effect on it.
3. When a listed module is absent, every **named** specifier of that
   import binds its local name as poisoned, with no diagnostic. A
   poisoned name types as `Type::Error` in expression position and
   resolves to `Type::Error` in type position, with no diagnostic in
   either. A default or namespace specifier keeps its S100.
4. Every other rule is unchanged. A diagnostic elsewhere still fails
   the check.
5. `hir::Module` gains `poisoned_imports: Vec<PoisonedImport>`, one
   per poisoned import statement in source order:

   ```rust
   pub struct PoisonedImport {
       /// The specifier as written in the import.
       pub module: String,
       /// `(imported, local)` name pairs in source order.
       pub names: Vec<(String, String)>,
       pub pos: Pos,
   }
   ```

   The vector is empty under `CheckOptions::default()`.
6. A module with a non-empty `poisoned_imports` is a discovery HIR.
   It can hold `Type::Error`. Both codegen entry points (`emit_c`
   and the dev-tier module lowering) return an error for it; the
   caller never lowers it.

### 63.2 Changes by site

- `compiler/src/lib.rs`: `CheckOptions`, `check_program_with`;
  `check_program` delegates. `check::run` takes the options.
- `compiler/src/check/mod.rs`: `ScopeItem::Poisoned`; the absent-
  module branch of `resolve_imports` consults the option; the
  record is assembled into the module.
- `compiler/src/check/expr.rs`, `compiler/src/check/tyres.rs`: the
  two lookup sites return `Type::Error` without a diagnostic for a
  poisoned name.
- `compiler/src/hir.rs`: `PoisonedImport`, the `Module` field.
- `codegen`: the guard in rule 6.

### 63.3 Tests and gate (pre-registered exit criteria)

No corpus entry: the surface is a Rust API, not language syntax.
Unit tests in `compiler/tests/` and `codegen/tests/`, same commit:

1. A program imports `{ A_SIZE, B_WGSL }` from an absent
   `./p.typegpu`, declares a `@CStruct` class and an exported
   function that uses both names (one as a value, one as a type).
   With the option: `Ok`, the class is intact,
   `poisoned_imports == [("./p.typegpu", [("A_SIZE","A_SIZE"),
   ("B_WGSL","B_WGSL")])]`. Without the option: `Err` with the S100
   in item 1.
2. `{ A as B }` records `("A", "B")`.
3. A listed module that is present resolves normally and the record
   is empty.
4. A second, unrelated diagnostic with the option set still returns
   `Err`.
5. `emit_c` and the dev-tier lowering reject a discovery HIR.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; every corpus count and
   golden unchanged.

## 64. R36 — async methods on generic classes, generic async functions

Origin: downstream request R36, 2026-08-23, at pin `bb9dadc`. The
downstream wraps a `GPUBuffer` in a generic class `Buffer<T>`. The
typed read-back belongs on that class and awaits a map, so the
method is `async`. §37.1 rejects an async method on a generic class
template (r104), and §26.1 accepts `await f(...)` only for a
directly declared, non-generic function.

Measurements at the pin, on this host:

1. A generic class with `async read(): Promise<T>`: S100 "async
   methods on generic class templates are not in the decided
   surface" (`collect_class`, `compiler/src/check/mod.rs`).
2. `await first<u32>(items)` for a generic `async function
   first<T>`: S100 "`first` is not a directly declared async
   function" (the await path in `compiler/src/check/expr.rs`
   accepts `ScopeItem::Func` only).
3. A non-generic class with an async method: accepted, runs.
4. `instantiate_fn` checks each instance as a function named
   `first<u32>` and marks it `exported` when the template is
   exported. The ship tier emits a sync instance of an exported
   generic `f<T>(): void` as `subscript_export_f_u32_`, a host entry
   that exists only while the program instantiates it. The runner
   kicks every exported async function with no parameters as a
   root (`subscript_kick_async_exports`, both tiers); an exported
   async instance would run twice.
5. Stock `tsc` 5.9.2 accepts an async method on a generic class, a
   generic async function with explicit type arguments, and an
   async arrow function. It rejects `async constructor()` (TS1089);
   the parser rejects it too ("Constructor can't be an async
   function").

### 64.1 Rule

1. An `async` method on a generic reference class is accepted. The
   class type parameters are in scope in the body and in the
   `Promise<T>` annotation. The body obeys §26.1 and §37.1
   unchanged. The §37.1 rejections that remain (async static, async
   generator, async on a `@CStruct` value class, `@Descriptor`,
   floating call) apply to the instance at instantiation, where
   every generic body check runs.
2. A generic `async function` is accepted. A call requires explicit
   type arguments, as every generic call does (S100 "generic
   function `f` requires explicit type arguments" when absent). The
   call is legal only in await position; a floating call is S013,
   as r100.
3. `await` accepts the two new forms wherever §26.1 and §37.1 accept
   the non-generic forms: `await f<A>(...)` and `await recv.m(...)`
   with `recv` of an instantiated generic class type. The await
   grammar gains no other form; async functions and methods stay
   non-first-class.
4. Each distinct type-argument list yields one instance, checked
   and lowered as a §26.2 async function or a §37.2 async method.
   The instance name is the monomorphized name (`first<u32>`); the
   ship tier sanitizes it as it does for sync instances.
5. An instance of a generic function is not a host entry and not an
   async root. `hir::Function.exported` is `false` on every instance,
   sync or async. The `export` keyword on a generic declaration
   affects module imports only. Both tiers emit no
   `subscript_export_*` symbol for an instance, and
   `subscript_kick_async_exports` kicks none.
6. Unchanged diagnostics: an async arrow function keeps S100 "async
   arrow functions are not in the decided surface; use an async
   function declaration"; `async constructor()` keeps the parse
   error.
7. r104 retires: the file and the harness row are removed, as
   `r14-async` in §26.1. §37.1's generic-class rejection and §26.1's
   "directly declared" wording are superseded by this section.

### 64.2 Checker and lowering

- `compiler/src/check/mod.rs` `collect_class`: the async-method
  rejection on generic templates is removed.
- `compiler/src/check/expr.rs`, the await path, identifier callee:
  `ScopeItem::GenericFunc(key)` requires type arguments, resolves
  them, calls `instantiate_fn`, and continues as the instance
  (`is_async` check, arguments, `AsyncCallee::Function(instance)`).
  The member arm needs no change: an instantiated generic class is
  a `Type::Class`.
- `compiler/src/check/mod.rs` `instantiate_fn`: passes `exported =
  false` to `check_function` (rule 5). The module export set for
  imports is unchanged.
- `compiler/src/language_reference.rs`: the Q34 prose drops
  "non-generic" and names the two new forms; the corpus list
  replaces r104 with a143 and r140; `generated-docs/` regenerates.
- No change in `codegen/`: the instance is an ordinary async
  function or method in HIR. Rule 5 removes the instance from the
  export and root sets through the `exported` flag.

### 64.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the two S100 in 64 items 1 and 2,
recorded (this host, exit 1).

1. `corpus/accept/a143-async-generic.ts` + `.expected`: `class
   Box<T>` with a field and `async read(): Promise<T>` that awaits
   `Context.suspend()` and returns the field; `async function
   first<T>(items: T[]): Promise<T>`; `export async function
   tick<T>(): Promise<void>` that prints once; both `Box` and
   `first` instantiated with `u32` and with a `@CStruct` value class
   (`Vec2`, two `f32` fields); `main` awaits each and prints the
   results through field reads. The golden shows the `tick` print
   once (rule 5). Golden from the dev JIT; ship byte-identical.
2. `corpus/reject/r140-async-lambda.ts`: an async arrow function in
   an async function body; S100 at the arrow; `tsc-clean-standalone`
   recorded in the header, as r100.
3. `corpus/reject/r104-async-generic-class-method.ts` is deleted;
   the harness row is removed.
4. Unit tests in the same commit: a floating `first<u32>(items)`
   call is S013; `await first(items)` without type arguments is
   S100; an async method on a generic `@CStruct` value class is the
   r103 S100 at instantiation; the HIR of a program with an
   exported generic async function has `exported == false` on the
   instance and the emitted C defines no `subscript_export_` symbol
   for it.
5. Counts: accept `.ts` 141 → 142; `.expected` 142 → 143; rejects
   135 → 135 (one removed, one added). The generated docs
   regenerate.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.

## 65. R37 — a named accessor is method sugar

Origin: downstream request R37, 2026-08-25, at pin `f99d4cb`. The
downstream compiles typed HIR to WGSL. Its surface reinterprets
TypeGPU, which reads a binding, an address-space variable, and a
vector swizzle through a property. subscript has no accessor, so
the downstream writes a call at 49 authored sites and in 3 library
classes. §58 already decided this shape for one sugar: the checker
rewrites the spelling to a call of a declared method, no new HIR
form exists, and no tier changes. R37 asks for the same treatment
of a named accessor.

Measurements at the pin, on this host:

1. The four probes reproduce. A `get`/`set` pair reports S100
   "static methods and accessors are not decided" once per
   accessor, then S004 and S100 at each use. A read accessor on a
   `@CStruct` value class reports the same S100. A static method
   reports the same S100. A method type parameter reports S100
   "unknown type name `T`". One message covers static members and
   accessors together.
2. Mirror ingestion rejects an accessor through that same shared
   message (`resolve_class_shape` runs for boundary classes). A
   split is therefore necessary, not optional.
3. The `$` collision is a live tier divergence, not a latent one.
   A class that declares methods `$` and `_` runs on the dev tier
   and prints `1,2`. The ship tier emits two definitions of
   `subscript_m0__` and the C compiler stops: "redefinition of
   'subscript_m0__'" (Apple clang). No corpus entry covers it. No
   identifier in `corpus/`, `examples/`, or `prelude/` holds a `$`
   today, so a new escape moves no emitted C.
4. `hir::DISPOSE_METHOD_NAME` is `"[[Symbol.dispose]]"`. A reserved
   HIR method name that no source identifier can spell is already
   the practice.
5. Stock `tsc` 5.9.2 accepts the whole asked accept surface: a
   `get`/`set` pair, a second accessor on the same class, a read
   accessor on a `@CStruct` class, an accessor on a generic class,
   and `$` and `_` members together. It also accepts `x.v += 1`,
   `x.v++`, the write used as a value, a static accessor, and a
   write accessor on a value class; each subscript rejection below
   is therefore a narrower pin. It rejects a write through a
   read-only accessor (TS2540) and a field that shares an accessor
   name (TS2300).
6. `private` fields check today. A synchronous method on a
   `@CStruct` value class checks and runs.
7. The reject harness checks one script file and cannot carry a
   mirror. The mirror rejection is a unit test, as §58.2 did.

### 65.1 Rule

1. A class declares a read accessor `get name(): T { ... }` and a
   write accessor `set name(value: T) { ... }`. Both carry a body.
   A read accessor declares no parameter and an explicit return
   type. A write accessor declares one parameter with an explicit
   type, no default on that parameter, and no return type. A return
   type on a write accessor fails with S100; stock `tsc` rejects it
   too (TS1095). A default on the parameter fails with S100; `tsc`
   rejects it too (TS1052). *(Amended 2026-08-25 after the phase
   review: the first text left both spellings unstated, and the
   implementation accepted a return type, which broke the
   `tsc`-subset invariant.)*
1a. The pair shares one type. The read accessor's return type and
   the write accessor's parameter type must be the same type. A
   mismatch fails with S100 at the write accessor. Stock `tsc`
   accepts unrelated types, so this is a narrower pin. *(Added
   2026-08-25 after the phase review: rule 1 wrote one `T` but
   nothing enforced it, so the written value took its context from
   the read accessor's type and a valid write reported S008.)*
2. A read accessor is legal on a reference class and on a
   `@CStruct` value class. A write accessor is legal on a reference
   class only. A write accessor on a value class fails with S100
   that names the value class. A value class copies on assignment,
   so the write reaches a copy.
3. An accessor adds no member kind and no HIR form. The class
   member namespace holds `name` once: a field, a method, or an
   accessor pair owns it. A second declaration of the name fails
   with S100 that names both member kinds. A class declares at most
   one read accessor and at most one write accessor of one name; a
   second one of either kind fails with S100 that names two
   accessors. *(Amended 2026-08-25 after the phase review: a second
   write accessor passed the first text's clash test, overwrote the
   first signature, and reached an internal lowering error.)*
4. The pair records as two ordinary methods. The read accessor
   records as the method `name` with no parameters. The write
   accessor records as the method `name=` with one parameter and
   the return type `void`. An identifier holds no `=`, so neither
   name collides with a declared method. *(This is the one
   divergence from the request, which asks for one method. A class
   method table holds one signature per name, and both tiers key a
   method by its name; two signatures under one name collide. The
   read call HIR is exactly the HIR of `x.name()`, and the write
   call HIR is exactly the HIR of `x.name=(v)`.)*
5. A read `x.name` checks to the same HIR as a call of the read
   accessor. A write `x.name = v` in statement position checks to
   the same HIR as a call of the write accessor. The value checks
   against the write accessor's parameter type by ordinary
   assignability (S007 on a mismatch).
6. A read accessor without a write accessor is legal. The write
   spelling then fails with S100 at the assignment. A write
   accessor without a read accessor fails with S100 at the
   declaration, because the read spelling has no target.
7. Rejected spellings, each with S100 and a message that names the
   spelling: compound assignment `x.name op= v`, increment and
   decrement on `x.name`, and the write used as a value. §58.1
   rule 7 rejects the same three for an index signature.
8. A static accessor keeps its rejection with its own message. The
   message for a static method keeps today's text. An accessor in
   a mirror class keeps its rejection with its own message.
9. An accessor on a generic class is legal. The checker checks each
   accessor body at instantiation, as it checks every other member
   body (§64.1 rule 1).
10. **Two distinct HIR names that share a C namespace must have
    distinct C identifiers.** `sanitize` in `codegen/src/cemit.rs`
    gains two escapes: `$` becomes `_dollar_`, and `=` becomes
    `_set_`. Every other character keeps today's mapping. An escape
    is not enough on its own, because a C identifier holds only
    `[A-Za-z0-9_]`, so every escape text is itself a legal source
    identifier. The emitter therefore holds one table for each C
    namespace it names into: the methods of one class, the fields
    of one class, the module's functions, the module's globals, and
    the parameters of one function. The first name keeps
    `sanitize`'s output. A later name whose output is already taken
    gains the smallest free `_N` suffix, with `N` from 2. The order
    is the HIR declaration order, so the assignment is
    deterministic, and a name that does not collide keeps its
    current C spelling. *(Amended 2026-08-25 after the phase review.
    The first text defined the escapes alone and accepted the
    residual. Measured: `get v` / `set v` beside an ordinary method
    `v_set_` runs on the dev tier and stops the C compiler with
    "redefinition of 'subscript_m0_v_set_'" — the divergence of 65
    item 3, reachable with no `$` at all.)* *(§66, 2026-08-25: a table over
    HIR names does not see an identifier the emitter mints or
    derives. §66 closes those two cases.)*
11. Nothing else moves. Fields, methods, and index signatures keep
    their rules. Static methods and method type parameters keep
    their rejections. Mirror ingestion reads no accessor.

`collisions.md` gains C12: JS runs an accessor on property access;
subscript calls the declared method.

### 65.2 Changes by site

- `compiler/src/check/mod.rs` (`resolve_class_shape`): the method
  arm splits the shared message. A static member reports "static
  methods and accessors are not decided" for a static accessor and
  keeps its other texts. An accessor in a boundary class reports
  its own S100. Every other accessor collects: the read accessor
  under `name`, the write accessor under `name=`. The arm validates
  the parameter count, the annotations, rule 2, rule 3, and rule 6.
- `compiler/src/check/mod.rs` (`ClassSig`): one new checker-side
  field records the accessor names of the class. `hir::ClassDef`
  gains nothing.
- `compiler/src/check/mod.rs` (`check_class_body`): the accessor
  bodies check as method bodies, under the names of rule 4.
- `compiler/src/check/expr.rs` (`member_on`): a `Type::Class`
  receiver whose class declares a read accessor `name` rewrites the
  read to the call, for a read and for a write target alike. The
  rewrite runs after the field lookup and before the
  method-as-value error.
- `compiler/src/check/expr.rs` (`check_assign`): a target that is a
  no-argument call of an accessor name rewrites the plain write to
  the write-accessor call. The rule 6 and rule 7 rejections report
  here.
- `compiler/src/check/expr.rs` (`check_update`): `x.name++` and
  `x.name--` report the rule 7 message.
- `codegen/src/cemit.rs` (`sanitize`): the rule 10 escapes.
- `compiler/src/language_reference.rs`: one feature entry for the
  accessor; `generated-docs/` regenerates.
- No runtime, prelude, or lowering change.

### 65.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 in 65 item 1 and the
ship-tier C error in 65 item 3, both recorded (this host).

1. `corpus/accept/a144-accessor.ts` + `.expected` (golden from the
   dev JIT; ship byte-identical): a reference class with a
   `get`/`set` pair over a private field, read, written, and read
   inside a template string, plus a second accessor on the same
   class; a `@CStruct` value class with a read accessor; a generic
   class with an accessor over its type parameter, used at two
   types; one class that holds both an accessor named `$` and a
   member named `_`; and, added 2026-08-25 after the phase review,
   one class that holds a `get`/`set` pair for `v` beside an
   ordinary method named `v_set_`, which pins rule 10 on the ship
   tier.
2. `corpus/reject/r141-value-class-write-accessor.ts`: S100 at the
   write accessor. `tsc`-clean, recorded in the header.
3. `corpus/reject/r142-readonly-accessor-write.ts`: S100 at the
   assignment.
4. `corpus/reject/r143-accessor-compound-assign.ts`: S100 at the
   compound write. `tsc`-clean, recorded in the header.
5. `corpus/reject/r144-accessor-increment.ts`: S100 at the
   increment. `tsc`-clean, recorded in the header.
6. `corpus/reject/r145-accessor-write-as-value.ts`: S100 at the
   write. `tsc`-clean, recorded in the header.
7. `corpus/reject/r146-accessor-field-name-clash.ts`: S100 at the
   second declaration.
8. `corpus/reject/r147-static-accessor.ts`: S100 at the accessor.
   `tsc`-clean, recorded in the header.
9. Unit tests in the same commit: an accessor in a mirror class
   fails with its own S100; a write accessor with no read accessor
   fails; a wrong-typed written value fails with S007; the checker
   produces identical HIR for `x.name` and for the spelled call of
   the read accessor; `sanitize` maps `$` and `=` to the rule 10
   escapes; the emitted C for a class with `$` and `_` members
   holds two distinct symbols and compiles. Added 2026-08-25 after
   the phase review: a return type on a write accessor fails; a
   default on the write accessor parameter fails; a read and a
   write accessor of different types fail; a second read accessor
   and a second write accessor each fail; the rule 10 table gives
   distinct C identifiers in each of the five namespaces.
10. Counts: accept `.ts` 142 → 143; `.expected` 143 → 144; rejects
    135 → 142; accept source files 144 → 145. The generated docs
    regenerate.
11. Gates: `cargo test --offline --workspace` in both profiles;
    zero-warning build; `cargo fmt --check`; the `tsc` gate; every
    pre-existing golden and `.expected` byte-identical; clippy
    library counts at the 7 / 22 / 29 baseline. The record quotes
    the test count and the wall time.

## 66. Emitted C identifiers — two spaces, never one

Origin: the R37 phase review found one collision between a declared
member and a symbol the emitter derives by suffix (§65 review note).
The audit that followed found the same defect class at function
scope, where it is **silent**. Owner decision 2026-08-25 to close the
class, not the instance. This is not a downstream request, and no
language surface moves.

Measurements at `a2228d9`, on this host. Every one is pre-existing;
§65 introduced none of them.

1. A parameter named `_t0` makes the two tiers disagree with no
   diagnostic. The program adds `g(i) + _t0` in a loop. The dev tier
   prints `306`. The ship tier prints `12`. A parameter named `_t1`
   prints `306` and `8`. The emitted body is:

   ```c
   static int32_t subscript_fn_f(void* ctx, int32_t _t0) {
       int32_t total = 0;
       {   int32_t i = 0;
           ...
           {   int32_t _t1 = total;
               int32_t _t0 = subscript_fn_g(ctx, i);
               total = ((_t1 + _t0) + _t0);
   ```

   `fresh_tmp` mints `_t0` inside a nested block. C block scoping
   makes it shadow the parameter, so the second `_t0` reads the
   temporary. Two declarations in one scope are a C error; two in
   nested scopes are silent.
2. An async method `x` beside a method `x_resume` stops the C
   compiler: "redefinition of 'subscript_m0_x_resume'". Both are
   file-scope definitions, so this one is loud. It stays loud when
   the two signatures are identical.
3. A method parameter named `_this` stops the C compiler:
   "redefinition of parameter '_this'".
4. `ctx` is already safe: `is_c_keyword` holds the C keywords and
   `ctx`, and a colliding name takes a trailing `_` (`ctx_`,
   measured). The mechanism exists. The list holds one entry that is
   not a C keyword.
5. The dev tier is immune. `codegen/src/lower/mod.rs` names a method
   `subscript_m{ci}_{mi}` by index. Only the C emitter mangles by
   name, so every divergence here is one-sided.
6. Audit of every symbol constructor in `codegen/src/cemit.rs`:
   `_resume` is the only symbol built from a source name by suffix.
   The lambda, bridge, worker-entry, constructor, and string-alias
   symbols are index-formed. `subscript_opaque` is emitted only for
   a class that declares no field.
6a. *(Corrected 2026-08-25 after the second phase review. The audit
   of item 6 read the function symbols and missed the frame **type**
   names.)* A coroutine frame type is built from a source name by
   **prefix**: `Async_m{class}_{method}` and `Async_{function}`, and
   the same two for `Gen_`. They share one C namespace. Measured: a
   free async function named `m0_x` beside an async method `x` on
   class index 0 both give `Async_m0_x`, and the C compiler stops
   with "redefinition of 'Async_m0_x'". Loud.
6b. A local inside a coroutine body resolves through `gen_locals`,
   which `local_ref` scans before every other stack. Such a local
   has no C identifier, so it is a rule 3a local. `emit_for_of`
   restores `gen_locals`; `emit_block` and `emit_for` do not.
   Measured, silent: an `async` function with `const s: string =
   "outer"` and an inner `const s: string = "inner"` prints `inner`
   then `outer` on the dev tier and `inner` then `inner` on the ship
   tier.
6c. An **unmanaged** local that shadows an outer **managed** local
   of one name does not mask it. `emit_let`'s unmanaged arm records
   no entry in `managed_scope`, while `emit_for_of_binding` records
   one for exactly this reason, so the outer entry wins the reverse
   scan for the rest of the block. Measured: `const s: string =
   "outer-string"` with an inner `const s: i32 = 5` prints
   `inner=5` then `outer-string` on the dev tier, and the ship tier
   stops with "incompatible pointer to integer conversion". The
   silent form of the same defect needs two types that are both
   `void*` in C.
6d. *(Third review, 2026-08-25.)* Two emitter defects survive that a
   scope restore cannot reach, because the emitter opens no C block
   where the language opens a scope. Both are on `tsc`-clean
   programs, so both are legal subscript programs that the dev tier
   runs and the C compiler rejects.
   - `emit_for_of` emits the loop binding and the body into one C
     block. A body local that shadows the binding is a redefinition.
     Measured: `for (const v of xs) { const v: i32 = 100; print(...)
     }` prints `100` three times on the dev tier, and the C compiler
     stops with "redefinition of 'v_v'".
   - `emit_lambda_fn` saves and restores the per-function stacks but
     not `assoc_iters`, which only `begin_fn` clears and a lambda
     never calls. A lambda created inside a `Map` or `Set` `for...of`
     emits the enclosing function's iterator handle in its own
     `return`. Measured: the dev tier prints `a10` and the C
     compiler stops with "use of undeclared identifier".
6f. *(Fourth review, 2026-08-25.)* A `for...of` **over a generator**
   reproduces the first defect of 6d, in the one `for...of` lowering
   `emit_for_of` never sees. `check_for_of` desugars that form to a
   `While` whose body holds the step, the break test, the binding,
   and the loop body in one flat HIR block, so both tiers emit one C
   block for all of it, while the checker gives the binding one
   scope and the body another. Measured, `tsc`-clean: `for (const v
   of numbers()) { const v: i32 = 100; print(...) }` prints
   `gen-for-of-body:100` twice on the dev tier, and the C compiler
   stops with "redefinition of 'v_v'". The seven fused `for...of`
   kinds — array values, array keys, `FixedArray` values, `Map`
   keys, `Map` values, `Set` values, and string code points — all
   agree across the tiers; the generator-driven form is the eighth
   and the only one that fails.
6g. *(Fifth review, 2026-08-25. The pairing is now exhaustive.)* The
   review paired every scope opener in the checker with every block
   site in the emitter. Checker: `ast::Stmt::Block`, `check_branch`
   (if-then, if-else, while body, `for` body, `for...of` body), the
   `for` head, the `for...of` binding, the `switch` case, and the
   lambda body. Emitter: the function bodies, `if`, `Block`,
   `while`, the `for` body, the `for...of` body, the `switch` case,
   the lambda body, and the coroutine resumes. Every pair opens a C
   block except two: the `switch` case, which measurement 6e records
   as an owner decision, and the lambda body.
6h. `emit_lambda_fn` materializes the capture copies and the body's
   own top-level locals in one C block, while the checker gives the
   lambda body its own scope and resolves a capture across that
   boundary. A body local whose name equals a capture is therefore a
   C redefinition. Measured, `tsc`-clean: a lambda that captures `n`
   from an enclosing `const n: i32 = 1`, holds an inner lambda
   reading the capture, and then declares `const n: i32 = 2`, prints
   `3` on the dev tier, and the C compiler stops with "redefinition
   of 'v_n'". This is the last unbraced pair of 6g.
6i. *(Sixth review, 2026-08-25. Recorded, not fixed here.)* The
   program of 6h reaches that collision only because name resolution
   diverges from TypeScript. The checker declares a name when it
   checks the declaration statement, so a lambda nested inside the
   body resolves the name outward to the enclosing binding.
   TypeScript and JavaScript block-scope the body, so the body's own
   `const` owns every reference in that body. Measured: `node`
   prints `4` and both subscript tiers print `3`. Stock `tsc`
   accepts the program, because the read sits inside a nested
   closure; a direct read reports TS2448. Under a TypeScript-faithful
   resolver the body binding owns the name, no capture of it is
   recorded, and the 6h collision cannot arise. The emitter fix
   stands on its own and is still correct. **Consequence for the
   corpus: no accept entry pins this shape.** The corpus is the
   language's executable definition, and a golden here settles
   a semantic divergence from TypeScript that no owner decision
   covers. The emitter unit test pins the C block without pinning a
   value. Name resolution needs its own request and its own owner
   decision.
6j. *(Sixth review, 2026-08-25. Recorded, not fixed here.)* The
   checker reports no duplicate declaration in one scope. Measured:
   `function f(n: i32): i32 { const n: i32 = 7; return n; }` prints
   `7` on the dev tier and stops the C compiler with "redefinition
   of 'v_n'"; two `const n` in one block, in a constructor, and in a
   method behave the same; the `async` form reaches an internal
   lowering error on the dev tier. Stock `tsc` rejects every one
   (TS2300, TS2451), so none is a valid subscript program under
   invariant 5. This is the bucket of measurement 6e — a
   checker-acceptance gap whose fix can move diagnostics — and it
   belongs to the same follow-up cycle.
6e. *(Third review, 2026-08-25. Recorded, not fixed here.)* The
   checker gives each `switch` case its own scope; TypeScript gives
   the whole switch body one scope. A program that declares a name
   in one case and reads it in another is accepted here and rejected
   by stock `tsc` (TS2454), which breaks invariant 5, and the flat C
   block then makes the two tiers disagree with no diagnostic.
   Measured: `case 1` prints `case1:1` on the dev tier and
   `case1:99` on the ship tier. The emitter is not the defect.
   Bracing each case in C is correct under only one of the two
   scope rules, so this section changes nothing here.

   **Owner decision 2026-08-25: the checker moves to the TypeScript
   rule — one scope for the whole `switch` body — in its own cycle,
   after §66 lands.** Two questions that cycle must settle from
   measurement, not from this note: `tsc` rejects the cross-case
   read with TS2454, which is a definite-assignment analysis this
   compiler may not have, so the cycle decides between implementing
   that analysis and taking a narrower rule that rejects a
   cross-case read outright; and once the body is one scope, the
   per-case scope restore in the emitter must go, because a name
   declared in one case is then legally in scope in a later one.
7. The emitter's own function-scope identifiers are `ctx`, `_this`,
   `_frame`, `_out`, `_f`, `_t{n}` (`fresh_tmp`), and `_L{n}`
   (`fresh_label`). A coroutine frame struct holds `_state`,
   `_this`, and `g{i}`.
8. The language permits shadowing in a nested block (measured: the
   inner `const x` prints `2`, the outer prints `1`). For a local
   that becomes a C variable, the emitter reproduces it with C block
   scoping: `local_ref` maps a name to `sanitize(name)` and tracks
   no scope. A fix must keep the C name a function of the source
   name alone.
8a. *(Corrected 2026-08-25 after the phase review. Measurement 8 was
   taken on an `i32` local and generalized to every local, which is
   wrong.)* A **managed** local — a string, a reference class, or an
   aggregate that holds a handle — has no C identifier. It lives in
   a shadow-frame slot, and `local_ref` resolves it by a reverse
   scan of `managed_scope`. `emit_block` saves and restores nothing,
   so an inner binding masks the outer one for the rest of the
   function and C block scoping never applies. Measured: `const s:
   string = "outer"` with an inner `const s: string = "inner"`
   prints `inner` then `outer` on the dev tier, and `inner` then
   `inner` on the ship tier, with no diagnostic. A reference-class
   local prints `2` then `1`, and `2` then `2`. `emit_for_of`
   already truncates both stacks on exit; `emit_block` does not.
8b. The lambda environment struct is a C namespace with no table.
   Its member is `sanitize(capture)` at the declaration, the store,
   and the read. Measured: a lambda that captures `a$b` and
   `a_dollar_b` emits `typedef struct { int32_t a_dollar_b; int32_t
   a_dollar_b; } EnvL0;` and the C compiler stops with "duplicate
   member". This one is loud.

### 66.1 Rule

1. **Every identifier the C emitter writes belongs to exactly one of
   two spaces.** Source space holds an identifier derived from an
   HIR name. Emitter space holds an identifier the emitter mints. No
   identifier belongs to both.
2. A function-scope source identifier — a parameter or a local —
   takes the prefix `v_`. The emitter mints no function-scope
   identifier that starts with `v_`. This separates the two spaces
   by construction where a collision is silent.
3. Within one function, two distinct source names take distinct C
   identifiers. The emitter holds one table per function over the
   parameter names and every local name in the body, in declaration
   order, and appends the smallest free `_N` on a collision, as §65
   rule 10. Two bindings of one source name keep one C identifier;
   C block scoping then reproduces the shadowing the language
   permits (measurement 8).
3a. **A local that has no C identifier obeys the same shadowing.**
   *(Added 2026-08-25 after the phase review; measurement 8a.
   Widened after the second review; measurements 6b and 6c.)* A
   managed local lives in a shadow-frame slot and a coroutine local
   lives in a frame field, so C block scoping cannot apply to
   either. The emitter's own scope bookkeeping supplies it. Every
   site that opens a lexical scope — `emit_block`, `emit_for`, and
   `emit_for_of` — records the lengths of **both** name stacks on
   entry, `managed_scope` and `gen_locals`, and truncates to them on
   exit. *(`local_types` was a third stack until the second review
   found it write-only; it is deleted.)* `shadow_cursor` and the frame let
   cursor stay monotonic, so no restored scope reads a stale slot.
3a-i. **Every binding masks a same-named binding from an outer
   block, whatever its storage.** An unmanaged local records an
   entry in `managed_scope` beside its C name, as
   `emit_for_of_binding` already does, so a `string` outside and an
   `i32` inside resolve to the inner one. Rule 3a's restore is what
   makes the mask end with its block.
3b. The lambda environment struct is one namespace and takes one
   table. *(Added 2026-08-25 after the phase review; measurement
   8b.)* The member declaration, the store in the creating frame,
   and the read in the lambda body all read it.
4. A symbol the emitter derives from a source name — by suffix or
   by prefix — takes its identifier from the table of the name it
   derives from, or drops the source name entirely for an index.
   *(Widened 2026-08-25 after the second review; measurement 6a. The
   coroutine frame types `Async_...` and `Gen_...` are
   prefix-derived and reached the same collision. A frame type takes
   the index form `Async_m{class}_{method index}`, which matches the
   dev tier's own convention and removes the source name from the
   derivation.)* The
   async **method** resume symbol is the only one:
   `{name}_resume` enters the class method table as a synthetic
   entry beside `{name}`, and the §65 rule 10 `_N` logic resolves a
   collision with a declared member. *(Clarified 2026-08-25 after
   the phase review.)* A free function's resume symbol is
   prefix-formed, `subscript_resume_{name}`, and no source name can
   reach that spelling, so it keeps it and takes no synthetic entry.
5. A coroutine frame struct is one namespace. `_state`, `_this`, and
   `g{i}` are emitter space. A parameter member is source space and
   takes rule 2's prefix.
6. Nothing else moves. A file-scope symbol keeps its `subscript_`
   prefix and its §65 rule 10 table. A class field member keeps its
   spelling and its §65 table. A module global keeps `g_` and its
   table. `is_c_keyword` keeps its list and its trailing `_`.
7. The rule is about emitted C only. No language surface and no
   diagnostic changes, and a source identifier keeps every spelling
   the language accepts today. *(Amended 2026-08-25 after the fourth
   review; measurement 6f.)* One HIR shape changes: `check_for_of`
   wraps a generator-driven loop body in `hir::Stmt::Block`, so the
   body is one scope in the HIR as it already is in the checker's
   own scopes. Both tiers and `rewrite_using_scope` already handle
   that statement, so `break`, `continue`, `return`, and scope-exit
   disposal keep their behaviour. No diagnostic and no accepted or
   rejected program moves.

§65 rule 10 gave each C namespace a table over HIR names. This
section adds the two cases a table over HIR names cannot see: an
identifier the emitter mints, and an identifier it derives.

### 66.2 Changes by site

- `codegen/src/cemit.rs` `emit_let` and `local_ref`: a local's C
  name comes from the per-function table of rule 3, with rule 2's
  prefix. `local_ref` keeps its shadow-frame and generator-frame
  arms, which name no C identifier of their own.
- `codegen/src/cemit.rs` `Emitter::new` and the per-function setup:
  the parameter table of §65 grows to cover the locals. `walk_lets`
  already collects the body's `let` names for the coroutine frame;
  the same walk builds the table.
- `codegen/src/cemit.rs` the coroutine frame emission: a parameter
  member takes rule 2's prefix, so it cannot collide with `_state`
  or `g{i}`.
- `codegen/src/cemit.rs` the method and function tables: an async
  member adds the synthetic `{name}_resume` entry of rule 4, and the
  resume signature and every use read the table.
- `compiler/src/check/stmt.rs` (`check_for_of`, the generator
  branch): the loop body becomes one `hir::Stmt::Block` instead of
  a flat extend (rule 7, measurement 6f).
- No runtime, prelude, or dev-tier change, and no other checker
  change.

### 66.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: measurements 1, 2, and 3 recorded
with their outputs (this host).

1. `corpus/accept/a145-emitted-identifiers.ts` + `.expected`: a
   program whose parameters and locals are named `_t0`, `_t1`,
   `_this`, `_frame`, `_out`, `_f`, `_state`, `g0`, `_L0`, and
   `ctx`, read inside nested blocks and inside a loop so the
   emitter's temporaries interleave with them; an async method `x`
   beside a method `x_resume`; an async function `f` beside a
   function `f_resume`; and an async function whose parameters are
   named `_state` and `g0`, so the frame struct exercises rule 5.
   Byte-exact across dev JIT, ship C-AOT, and the golden. The entry
   is `tsc`-clean, like every accept entry.
2. Unit tests in the same commit: a parameter and a local carry the
   rule 2 prefix in emitted C; the emitted C for measurement 1's
   program declares no name that equals a parameter name; the
   synthetic resume entry resolves against a declared `x_resume`;
   the per-function table gives distinct names to `a$b` and
   `a_dollar_b` as two locals; the frame struct of an async function
   with a parameter named `_state` holds two distinct members. Added
   2026-08-25 after the phase review: an environment struct for a
   lambda that captures `a$b` and `a_dollar_b` holds two distinct
   members; `emit_block` restores both scope stacks, so a managed
   local declared in a nested block does not mask the outer one.
2a. Corpus entries added 2026-08-25 after the phase review, in
   `corpus/accept/a146-scoped-locals.ts` + `.expected`: a `string`
   local and a reference-class local, each shadowed in a nested
   block and read again after it; a shadowed managed local inside a
   loop body and inside an `if` branch; and a lambda that captures
   `a$b` beside `a_dollar_b`. Byte-exact across dev JIT, ship
   C-AOT, and the golden. This entry is the gate for rules 3a and
   3b; without it the corpus does not see them.
2b. Widened 2026-08-25 after the second review, because the item 2a
   list is what missed measurements 6b and 6c. `a146` also holds:
   the same shadowing inside a generator body and inside an `async`
   body across a suspension; a shadowed local in a `switch` case
   and in a `for...of` body; an unmanaged local that shadows an
   outer managed local of one name, and the reverse; and a free
   async function named `m0_x` beside an async method `x` on the
   first class, which pins measurement 6a. Every construct that
   opens a lexical scope appears at least once.
2c. *(Third review, 2026-08-25.)* Item 2b's shapes must appear
   **outside** a coroutine as well. In the entry as first written,
   the `switch` case and the `for...of` body sat inside a generator,
   where every local is a frame field — the one storage class that
   shows neither defect of measurement 6d. `a146` must hold, outside
   any coroutine: a `for...of` body that declares a local shadowing
   the loop binding; a `for` **body** declaration, not only a
   shadowed initializer; a lambda **body** with its own locals,
   including a lambda created inside a `Map` or `Set` `for...of`; a
   constructor body, a method body, and an accessor body; and a
   `using` scope. A construct that no corpus entry can express,
   because `tsc` rejects it, is named in measurement 6e instead.
2d. *(Fourth review, 2026-08-25.)* `a146` also holds a `for...of`
   **over a generator**, outside any coroutine, whose body declares
   a local that shadows the loop binding. The seven fused `for...of`
   kinds were all measured to agree; the generator-driven form is a
   separate lowering and needs its own line in the entry.
2e. *(Fifth review, 2026-08-25; corrected by the sixth,
   2026-08-25.)* `a146` holds, outside any coroutine, a shadowed
   local inside one `switch` case, which item 2b named and item 2c's
   list left out. It does **not** hold a lambda body local that
   shadows one of the lambda's own captures: measurement 6i shows
   that program's value depends on a name resolution that diverges
   from TypeScript, so a golden would settle that divergence without
   a decision. The emitter unit test
   `lambda_body_uses_a_nested_c_scope_below_capture_copies` pins the
   C block of measurement 6h instead, with no value. A shadow inside one case is expressible and was
   measured to agree on both tiers; only the cross-case read of
   measurement 6e is not expressible.
3. Counts: accept `.ts` 143 → 145; `.expected` 144 → 146; accept
   source files 145 → 147. Rejects do not move. The generated docs
   regenerate.
4. **Every committed golden and `.expected` stays byte-identical**,
   `a145` excepted as a new entry. Rule 2 moves emitted C text, not
   program output. The `codegen/tests/cemit.rs` assertions that pin
   body text move with it, in the same commit; that is expected, and
   each move must be a prefix change and nothing else.
5. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. The record quotes the
   test count and the wall time.

### 66.1 The emitted spelling is not an interface

*(2026-08-28. A consumer asked whether `SubC{id}` and `d{id}` are
contract, because it compiles a C probe that names both.)*

**They are not, and no consumer must name them.**

§68 requires that a C identifier derives from a LIR id, so that no
source name reaches the C namespace. That rule is what closes §66's
collision class. The spelling that satisfies it is a consequence, not
a promise.

`cemit.rs` builds a class name as `format!("SubC{}", id.0)`, and
`ClassId` is `pub struct ClassId(pub usize)` — an index into the
module's class table. Adding, removing, or reordering a class moves
every later id, so the same source class takes a different C name in a
different program. The spelling is stable for one compilation, and for
nothing wider.

**This project names no emitted identifier to prove anything.** §12.3's
`offsetof` proof compiles its probe against the C header's own type
names, and compares the result against the layout the compiler
computed. It never reads emitted C.

**A consumer proves the same property the same way.**
`subscript_codegen::layout::value_class_layouts` is public. It returns
one `StructLayout` per `@CStruct` class — `name`, `size`, `align`, and
one `FieldLayout { name, offset }` per field — keyed by the **source**
class and field names. The caller compares those against
`sizeof`/`_Alignof`/`offsetof` taken from its own header. Both sides of
that comparison are names the consumer controls.

## 67. Checker scoping, and state that must survive a suspension

Origin: the §66 arc recorded three constructs it could not fix
(measurements 6e, 6i, 6j), and its reviews found four more defects
outside its subject. Owner decision 2026-08-25: **all seven land in
one cycle, in two passes.** One contract, two implementation and
review passes, because one review does not cover that surface —
the lesson of §66's own seven rounds.

Pass A is checker semantics: what the language accepts. Pass B is
lowering: what an accepted program does. The two passes share no
code and no corpus entry.

Measurements at `a239de7`, on this host. Every one is pre-existing.

Pass A. Each program is one stock `tsc` rejects and this compiler
accepts, so each breaks invariant 5.

1. A `switch` case reads a name a earlier case declared. Measured:
   the dev tier prints `case1:1` and the ship tier prints
   `case1:99`, with no diagnostic. `tsc` reports TS2454. The checker
   gives each case its own scope; TypeScript gives the whole switch
   body one scope, and the emitter writes one flat C block.
2. A function parameter and a body local of one name. Measured: the
   dev tier prints `7`; the ship tier stops with "redefinition of
   'v_n'". `tsc` reports TS2300.
3. Two `const` of one name in one block. Measured: the dev tier
   prints `2`; the ship tier stops with "redefinition of 'v_n'".
   `tsc` reports TS2451.
4. A lambda body declares a name that a nested lambda reads earlier
   in the same body. Measured: `node` prints `4`; both tiers print
   `3`. `tsc` accepts, because the read sits inside a closure; a
   direct read reports TS2448. **The two tiers agree here**, so this
   one is not a tier divergence. It is a divergence from TypeScript
   semantics, and it is the only measurement in this section whose
   fix can change what an accepted program prints.

Pass B. Each program is `tsc`-clean, so each is a valid subscript
program that does not run correctly.

5. Two `await` expressions in one expression. Measured: the dev
   tier stops with "internal lowering error: define async resume:
   Compilation(Verifier(... uses value v27 from non-dominating
   inst38))"; the ship tier runs and prints a corrupt first value.
6. A capturing lambda created before a suspension and called after
   it. Measured: the dev tier prints `15` then `-1927167400`; the
   ship tier prints `15` then `5`. The correct output is `15` twice.
   Both tiers are wrong, and they disagree.
7. `await` inside a `for...of` body. Measured: the dev tier stops
   with the same verifier error as item 5; the ship tier prints one
   iteration and stops. `yield` inside a `for...of` body fails the
   same way.
8. Two generators of one yield type in one module. Measured: the
   dev tier prints `1` then `2`; the ship tier refuses to emit,
   with "ambiguous generator resume target".

Items 5, 6, and 7 are one root cause: **a value that is live across
a suspension does not live in the coroutine frame.** The Cranelift
verifier names it exactly — a value defined before the suspension
is used after it, in a block the definition does not dominate. Item
8 is a separate defect: `generator_of`
(`codegen/src/cemit.rs`) recovers the target by searching for the
one generator whose yield type matches, and its own comment records
that it cannot recover the creator. The dev tier has no such search:
it stores the resume address in the frame at creation
(`codegen/src/lower/func.rs`, `GEN_RESUME_OFF`) and calls through
it.

### 67.1 Pass A rules — narrow, never diverge

The guiding rule: where this compiler and TypeScript disagree, this
compiler **rejects**. It never accepts a program and gives it a
different value. Rejecting more is inside invariant 5; computing a
different answer is not.

*(Clarified 2026-08-26 after the pass A review. Invariant 5 runs one
way: an accepted program must type-check under stock `tsc`. It does
not say a `tsc`-clean program must be accepted. This language is a
subset, and 23 reject entries already carry a
`tsc-clean-standalone` header. A rule that rejects a `tsc`-clean
program is therefore ordinary, and its entry records the `tsc`
verdict in the header. The pass A handoff stated the converse by
mistake, and the round put the weaker `+=` spelling in the corpus to
satisfy it.)*

1. A `switch` body is one scope. A declaration in one case is in
   scope for the whole body, as TypeScript has it. Two declarations
   of one name in one switch body fail with S100 that names the
   switch, matching the direction of TS2451.
1a. **The scope owns the disposal.** *(Added 2026-08-26 after the
   second pass A review.)* A `using` declaration in a case belongs
   to the switch body, so §60's hook runs at each exit of the switch
   body, in reverse declaration order across every case. Measured:
   `using a` in `case 0` falling through into `using b` in `case 1`
   prints `case0 / case1 / dispose:b / dispose:a / end` under
   TypeScript (downlevelled to ES2022, `node` v24.18.0), while this
   compiler printed `case0 / dispose:a / case1 / dispose:b / end` on
   both tiers. Rule 1 moved the scope and left the disposal site
   behind, so the round created that inconsistency.
2. A read of a name that a **different** case declares fails with
   S100 at the read. TypeScript rejects the same program through a
   definite-assignment analysis (TS2454) that this compiler does not
   have; this rule is narrower and needs no such analysis.
3. Two declarations of one name in one scope fail with S100 at the
   second declaration. A function parameter and a body local of one
   name are two declarations in one scope, because the body opens no
   scope of its own.
3a. **A class body is one member namespace: one name, one member.**
   *(Owner, 2026-08-29: "A で進めて", after the Fable phase review of
   §66–§67, M1.)* Rule 3 is stated for a scope; a class body is the
   scope of its instance members, and §65 applied the rule to
   accessors only. Measured at `857757a`, `tsc` under the corpus
   gate's options:

       field + method     checker accepts; c.x prints 1, c.x() prints 2     tsc TS2300 ×2
       method + method    checker accepts; the lowering fails internally    tsc TS2393 ×2
                          ("class `C` has duplicate checked method `x`")
       field + field      checker accepts; prints 2, the later wins         tsc TS2300
       field + accessor   S100 (§65)                                        tsc TS2300 ×2
       static + instance  S100, static fields are undecided                 tsc accepts

   The checker resolved a member by its use — a read found the field,
   a call found the method — and accepted programs `tsc` rejects, which
   invariant 5 forbids. The method pair reached the lowering, which
   reports an internal error where a diagnostic belongs.

   **Rule.** Two instance members of one name in one class body —
   a field, a method, or an accessor pair, in any combination — fail
   with S100 at the second declaration, naming both kinds. §65's
   accessor rule is one case of this rule and keeps its message. The
   static namespace is separate, as `tsc` has it; static fields stay
   undecided under their own S100.

   No accept entry has a clashing class (measured, 0 of 165). Corpus:
   `r161` (field and method), `r162` (two methods), `r163` (two
   fields), each with the measured `tsc` code in its header.
4. A block-scoped declaration owns its name for the whole block. A
   read of that name earlier in the same block fails with S100 at
   the read, whether the read is direct or inside a nested lambda.
4a. **Rules 2 and 4 reach every name, not only a local.** *(Added
   2026-08-26 after the second pass A review.)* A declaration owns
   its name against an ambient namespace and against a class name
   too. Measured, each accepted here and rejected by `tsc`:
   `Math.abs(-2.5)` before `const Math: i32 = 3` printed `2.5:3`
   (TS2448, TS2454); `new Foo()` before `const Foo: i32 = 9` printed
   `1:9` (TS2351, TS2448, TS2454); the same two shapes across two
   switch cases behaved the same. Every site that asks whether a
   name is shadowed must consult the pending declarations and the
   switch declarations, not the bound locals alone.
   TypeScript accepts the closure form and rejects the direct form
   (TS2448); this rule rejects both, so **no accepted program
   changes its value**. Measurement 4's program becomes a rejection.
4b. **A local that owns a class name hides the class from that
   point.** *(Added 2026-08-26 after the third pass A review, which
   found the rule already widened this far while 4a described only
   the read-before-declaration shape.)* `const Foo: i32 = 9` before
   `new Foo()` fails with S100. Stock `tsc` rejects the same program
   (TS2351), so the direction is right. The message names the
   shadow. It must not say the class is unknown, because the class
   is declared and a local owns its name.
4c. **Module scope: an initializer reads only what is already
   initialized.** *(Added 2026-08-28 after the Fable phase review of
   §66–§67, finding C1.)* Rules 3 and 4 stop at the function boundary
   and did not say so. `reserve_block_declarations` runs for every
   function, lambda, block, branch, and switch body, and never for the
   module. Measured at `2a65724`, `tsc --strict` accepts this program:

   ```ts
   class Box { value: i32 = 5; }
   const g: Box = f();
   function f(): Box { return h; }
   const h: Box = new Box();
   export function main(): void { print(`h=${h.value}`); print(`g=${g.value}`); }
   ```

       subscript check   no error
       dev tier          signal 11
       ship tier         h=5, then SIGSEGV
       node              ReferenceError: Cannot access 'h' before initialization

   With `i32` in place of `Box`, both tiers print `0:1` and no
   diagnostic. With `string`, the null string prints nothing. The
   direct form `const a: i32 = b; const b: i32 = 2;` is accepted here
   too and prints `0:2`; `tsc` rejects it (TS2448).

   **Rule.** Module-level bindings initialize in declaration order. A
   module-level initializer reads or writes, directly or through any
   function it calls, only a module-level data binding declared before
   it. A violation fails with S100 at the initializer. The message
   names the binding and the call path that reaches it.

   The check is a fixpoint over the module's functions. A function's
   global set is its direct global reads and writes, plus the global
   sets of the functions it calls. An indirect call through a function
   value reads every global. A `function` declaration is hoisted and
   is not a binding for this rule. A class name is not a binding. A
   `const`, `let`, or `using` at module level is.

   **Why reject, not trap.** Invariant 6 asks for a clear, early
   error. A trap needs a null check at every read of a global
   reference, and every program pays it for a defect only a module
   initializer can cause. `tsc` accepts the function-mediated form and
   `node` throws; this compiler narrows (invariant 5). **No accepted
   program changes its value**: measured at `2a65724`, zero accept
   entries hold a calling module-level initializer followed by a later
   data binding.

   Corpus: `r158` (the direct form; `tsc` rejects, TS2448), `r159`
   (the function-mediated form; `tsc` accepts, `node` throws), `a160`
   (the legal shapes: an initializer that reads an earlier binding
   directly and through a function; a function declared after the
   initializer that reads only earlier bindings; a later binding read
   only from `main`).
5. Nothing else moves. Every other accepted program keeps its
   diagnostics and its output.
6. §66 measurement 6e's note applies: once a switch body is one
   scope, the emitter's per-case scope restore must go, because a
   name a case declares is then in scope in a later case.

### 67.2 Pass B rules — the frame holds what outlives a suspension

1. **Every value that is live across a suspension lives in the
   coroutine frame.** This holds for a temporary inside a composite
   expression, for a lambda environment, and for the loop state of a
   `for...of`. Neither tier keeps such a value in a register, on the
   C stack, or in a frame that the resume abandons.
1a. *(Added 2026-08-26 after the pass B review, which found rule 1
   implemented at four sites and measured seven `tsc`-clean shapes
   that still lose a value.)* "Every value" is the whole rule, not a
   list. The sites the first round missed, each measured: an array
   literal whose element suspends; a `new` whose argument suspends;
   a method receiver evaluated before a suspending argument; an
   assignment target resolved before a suspending right side; and
   the pre-read of a compound assignment. `xs[1] += await a()` is
   the worst of them — the dev tier stops with the verifier error
   and the ship tier prints `xs=1,2` where `xs=1,5` is correct, with
   no diagnostic.
1b. **The frame holds a slot only for a value that is live across a
   suspension.** *(Added 2026-08-26.)* The first round reserved one
   slot for every expression node in the body: an async function of
   100 statements with one `await` emitted a 3636-byte arena, about
   36 bytes per statement, in a per-invocation Context allocation.
   The planner reserves where a later sibling can suspend, and it
   reuses a slot whose value is dead.
1d. **The plan and the two lowerings walk in one order, and a test
   proves it.** *(Added 2026-08-26 after the second pass B review.)*
   The ship tier needs the frame layout before it emits the body, so
   a pre-pass is structural. That pre-pass and each tier's lowering
   consume one event list through a strict cursor, so any
   disagreement about order or kind is a hard error at compile time
   and a wrong slot at run time. Measured: the planner visited a
   `for` as init, cond, step, body while both tiers lower init,
   cond, body, step, and it visited a `switch` as discriminant then
   test-and-body pairs while both tiers emit every test and then
   every body. Both mismatches refused ordinary programs. A unit
   test asserts that the plan's event kinds equal each tier's
   request sequence, for every statement form.
1g. **Each tier consumes the whole event list, and a short cursor is
   a compile error.** *(Added 2026-08-26 after the third pass B
   review, which is the mechanism this arc lacked.)* The cursor is
   strict on kind but nothing asserted that it reached the end of a
   coroutine body, so a site the planner reserved for and a tier
   never spilled was silent. The planner walks every callee kind, so
   it already reserves for a site no tier closed: the check turns
   that reservation into an error the compiler reports, and it turns
   the search for unclosed sites from a review's guesswork into a
   corpus run. Measured before the check existed: a frame declared
   `spill0`, `spill1`, and `spill2` while the emitted body wrote
   only `spill0`, and the two unwritten slots were a foreign call's
   marshalled arguments, which the ship tier then read as garbage —
   `probe=4347879728` where `probe=2` is correct, with no
   diagnostic. Each tier asserts at the end of a coroutine body that
   the spill cursor and the lambda cursor are both exhausted.
1e. **Correctness before size.** *(Added 2026-08-26 after the second
   pass B review.)* Rule 1b narrows what the frame holds. A
   narrowing is admissible only when the same traversal that
   assigns slots proves the value dead, and a narrowing that is
   wrong is a silent wrong answer, not a larger frame. Measured
   after the first narrowing landed: a lambda captured before a
   loop and called after a suspension inside it printed `15` then
   two garbage values on the dev tier and `15` then two zeroes on
   the ship tier, because the scan walked the loop body once and
   missed the back edge; and a lambda reached by assignment rather
   than by `let` got no frame environment at all. When liveness is
   in doubt, reserve.
1f. **A slot's live range is the value's, not the statement's.**
   *(Added 2026-08-26 after the second pass B review.)* Measured:
   two lambda environments of one capture shape, both live across
   suspensions, shared one frame member, so an inner environment
   overwrote an outer one and both tiers printed the inner value
   twice. *(Widened after the third review.)* The live range
   is the range of the **local that holds the value**, not of the
   statement list that contains the literal. Measured: a lambda
   assigned to an outer local from inside a nested block kept its
   environment in a C block-local the frame abandons, so both tiers
   printed a wrong number and disagreed with each other; and a
   second lambda of one capture shape reused the member of a first
   that a nested block had assigned to an outer local, so both tiers
   agreed on a wrong answer, which the differential gate cannot
   see.
1c. **A spill slot is a typed frame member.** *(Added 2026-08-26.)*
   The first round emitted one untyped byte array and read it as
   `(*((T*)(void*)(_f->_spill + N)))`. The ship tier compiles with
   `-std=c11 -O2` and no `-fno-strict-aliasing`, so reading an
   `unsigned char[]` object through an incompatible lvalue is
   undefined (C11 6.5p7). The frame already carries typed members
   for the `let` declarations; a spill slot takes the same form.
   That also removes the offset arithmetic and the alignment
   round-up the two tiers computed differently.
1h. **Liveness is a property of evaluation order, not of source
   order.** *(Added 2026-08-26 after the fourth pass B review.)* The
   scan that decides whether a value is live across a suspension
   walks the same traversal that the planner walks to emit events.
   A scan that walks HIR source order treats a read that precedes
   the suspension in the text but follows it in evaluation as dead.
   Measured, each a wrong answer with no diagnostic and a tier
   disagreement: a lambda called with a suspending argument, where
   the callee is read first and used at the call — the dev tier
   printed `s02=1929953583` and the ship tier printed `s02=3` where
   `s02=10` is correct; a capturing lambda passed as an argument
   beside a suspending argument — `x01=-1777237247` and `x01=1`
   where `x01=15` is correct; and a `default` arm written before a
   suspending `case` test, which the switch reaches only after every
   test has run — `P2=-519027216` and `P2=0` where `P2=15` is
   correct. Three shapes of one root cause. Rule 1e already says
   that a doubtful liveness reserves; a scan that cannot see the
   read is not in doubt, so the rule needs the traversal, not more
   cases.
1i. **One function computes a spill's kind, and the planner and both
   tiers call it.** *(Added 2026-08-26 after the fourth pass B
   review.)* The planner took the kind from the expression type; the
   dev tier took it from the declared type at four sites — a
   parameter type, a function-type parameter, and two `FixedArray`
   element types. Where the two differ, the strict cursor of rule 1d
   refuses a `tsc`-clean program that the ship tier compiles and
   runs correctly. Measured: `take(null, await av(3))` against
   `function take(b: Box | null, n: i32)` stopped the dev tier with
   "coroutine spill event mismatch: planned Value(Null), lowered
   Value(Nullable(Class(ClassId(0))))" while the ship tier printed
   `P1=3`, which is correct. A strict cursor is a check on agreement,
   not a source of it.
1j. **The receiver of a suspending async call is a spill site both
   tiers close.** *(Added 2026-08-26 after the fourth pass B
   review.)* The planner reserves for it and neither tier consumes
   it, so rule 1g refuses the program. Measured: `await
   m.step(await av(5))` stopped both tiers with "coroutine spill
   cursor stopped at 1/2" where `P8=15` is correct. This is the one
   unclosed site that remained after rule 1g landed, and rule 1g
   named it rather than a review finding it.
1k. **A capturing lambda created inside a coroutine always holds
   its environment in the frame. No liveness test decides it.**
   *(Added 2026-08-26 after the fifth pass B review. This rule
   deletes machinery; it does not add any.)* Rounds 2 to 5 each
   narrowed the reservation and each narrowing was wrong in a new
   way. The fifth review measured four more holes in one scan, and
   two of them are not holes but boundaries: a lambda passed to a
   coroutine callee is used after the **callee's** suspension, which
   an intraprocedural scan cannot see; and liveness through a
   capture is transitive, because a lambda that captures a lambda
   keeps a pointer to the second environment. Measured, each a wrong
   answer with no diagnostic and a tier disagreement: a lambda
   passed to an async callee that suspends before it calls it — the
   dev tier printed `p6=399051668` and the ship tier printed `p6=0`
   where `21` is correct, and AddressSanitizer named it
   `stack-use-after-return`; a lambda whose destination local is
   declared in a `for` initializer, whose scope the trace closes
   before the body — `after=-665124504` and `after=0` where `21` is
   correct; a chained assignment `h1 = h2 = <lambda>`, where the
   environment takes the outermost target's binding — `h2only=
   1905131784` and `h2only=0` where `18` is correct; and a lambda
   reached only through another lambda's captures —
   `after=-761642027` and `after=1` where `21` is correct. Rule 1e
   says that a doubtful liveness reserves. Four rounds of evidence
   say this liveness is always doubtful, so the test goes. Rule 1b's
   size measurement was taken on expression spill slots, not on
   lambda environments, and it does not carry here. The liveness
   narrowing stays for expression spill slots, where the strict
   cursor of rule 1d proves the trace against both tiers.
1l. **A statement the lowering skips still consumes its planned
   events.** *(Added 2026-08-26 after the fifth pass B review.)* The
   dev tier stops at a terminator and skips the rest of a statement
   list. The trace walks the skipped statements, so rule 1g's
   end-of-body check reports events no tier consumed, and the dev
   tier refuses a program the ship tier compiles and runs.
   Measured: a `return;` followed by `xs.push(await av(2))` stopped
   the dev tier with "coroutine spill cursor stopped at 0/1" while
   the ship tier printed `start`, which is correct, and both tiers
   printed `start` at the pin. This is a regression that rule 1g
   introduced. Either the lowering advances every cursor across a
   skipped statement, or it does not skip. The check must not
   change which programs compile.
1m. **The operand tables changed a synchronous program, and that
   is rule 7 again.** *(Added 2026-08-26 after the sixth pass B
   review.)* The dev tier evaluates every argument into a table and
   then pushes them, so an aggregate operand is copied after a
   later operand has run. A later operand that overwrites the
   aggregate wins. Measured on a program that holds no `async` at
   all: `sink(h1.v, bump(h1))`, where `bump` overwrites `h1.v`,
   printed `call=199` on the dev tier and `call=31` on the ship
   tier, and `31` is correct and is what the pin printed on both
   tiers. The same shape reproduces on a `FixedArray` argument, on
   an indirect call, on a constructor, and on an array literal. The
   copy of an aggregate operand happens at the operand, not at the
   call. Rule 7 states the requirement; this rule names the second
   site that broke it.
1n. **A boundary struct's `new` is a spill site both tiers close.**
   *(Added 2026-08-26 after the sixth pass B review.)* The planner
   reserves for the receiver and for each argument that a later
   suspension outlives. The boundary branch of each tier stores
   arguments positionally and consumes nothing, so rule 1g refuses
   the program. Measured on `new SubRect(1, await av("rect-y", 2),
   3 as u32, 4 as u32)`: the dev tier stopped at "cursor stopped at
   1/2" and the ship tier at "cursor stopped at 0/2", where the pin
   compiled, linked, and printed `rect=1,2,3,4` on the ship tier.
   Rule 1l applies: the check must not change which programs
   compile.
2. Two `await` expressions in one expression are legal, and each
   operand evaluates once, left to right, with the earlier result
   held in the frame across the later suspension.
2a. **The ship tier allocates a suspension's label number after it
   emits the operands, not before.** *(Added 2026-08-26 after the
   fourth pass B review.)* `eval_async_call` read the yield counter
   before it emitted the argument list, so a nested `await` in that
   list took the number the outer call had already claimed. Measured
   on `await ai(await av(2))`: the dev tier printed `s01=3`, which
   is correct, and the ship tier stopped the C compiler with
   "redefinition of label '_gresume0'". The defect predates this
   section; the dev tier failed on the same program at the pin, so
   the two tiers agreed by both failing. Rule 2 states that the
   program is legal, so this section fixes it.
3. A capturing lambda created before a suspension and called after
   it reads the values it captured. Its environment lives in the
   frame.
4. `await` and `yield` inside a `for...of` body are legal. The loop
   subject, the index, and the bound live in the frame.
5. The ship tier dispatches a generator resume through the frame, as
   the dev tier does. `generator_of`'s search by yield type is
   deleted. Any number of generators of one yield type is legal, and
   a generator handle passed to a function resumes correctly.
6. Both tiers agree byte for byte on every program above.
7. **A program that does not suspend keeps its output.** *(Added
   2026-08-26 after the fourth pass B review.)* This section changes
   coroutines. It does not change the evaluation or the marshalling
   order of any other program. Measured: the round evaluated every
   operand of a foreign call before it marshalled any argument, for
   every call, because the pre-evaluation is keyed on the callee
   kind and not on the presence of a suspension. An array argument's
   data pointer and count are then read after a later argument has
   run. A later argument that grows the array moves its storage.
   Measured on a call whose third argument pushes to the array of
   the second: the pin printed `f2=2` and the round printed `f2=3`.
   The comment at the marshalling site states the old order and the
   reason for it, and the round left the comment in place. The order
   at the pin holds. If a suspension in a later argument makes the
   old order impossible, the round reports the conflict and changes
   nothing; the choice is not the round's.
7a. **The conflict of rule 7 is real, and it is recorded here rather
   than decided by a round.** *(Added 2026-08-26 after the fifth
   pass B review.)* The round did not report the conflict, and it
   changed the suspending case. Measured on two programs that differ
   only in whether the third argument suspends: the plain call gives
   `f2sync=2 len=3` and the suspending call gives `f2suspend=3
   len=3`, on both tiers. At the pin the suspending program did not
   run at all, so `3` is a new value, not a restored one. A
   marshalled array pointer and count cannot survive a suspension,
   because a collection moves the storage, so the pin's order is
   unavailable when a later argument suspends. The behaviour stands
   as measured and a corpus entry pins both twins. **Owner decision
   open:** whether a foreign call whose later argument suspends is
   legal at all, or is a compile error, or keeps this order. Until
   the owner decides, the compiler keeps this order and the entry
   records that the value is not settled.

   **Decided 2026-08-28: the call-time view** (§68.7.3, the Foreign
   row). The data pointer and count are read after every argument and
   immediately before the call, so `f2sync` and `f2suspend` both give
   `3`. The suspending call is legal. Rule 7's "before a later
   argument runs" is withdrawn for foreign array arguments.

### 67.3 Changes by site

Pass A, `compiler/src/check/`: the `switch` case scope becomes one
scope for the body (`stmt.rs`); `fx.declare` reports a duplicate in
one scope (`mod.rs`); a block records its declarations before it
checks its statements, so a read earlier in the block resolves to
the later declaration and reports; the lambda body check consults
that record across the closure boundary. `codegen/src/cemit.rs`
drops the per-case scope restore (§66 rule 6e).

Pass B, `codegen/src/lower/func.rs` and `codegen/src/cemit.rs`: the
frame gains a slot for every value live across a suspension, and
both tiers spill and reload it; the lambda environment of a
capturing lambda inside a coroutine becomes a frame field; the
`for...of` loop state becomes frame fields; the ship tier stores the
resume address in the frame at creation and calls through it, and
`generator_of` is deleted.

### 67.4 Corpus and gate (pre-registered exit criteria)

Red first, per pass, at the contract pin: the measurements above,
recorded with their outputs.

Pass A:

1. `corpus/reject/r148-switch-cross-case-read.ts`,
   `r149-switch-duplicate-declaration.ts`,
   `r150-parameter-and-local.ts`, `r151-duplicate-const.ts`, and
   `r152-read-before-declaration.ts` — the last one in the nested
   lambda form of measurement 4. Each pinned by code and line. None
   carries a `tsc-clean-standalone` line, because `tsc` rejects the
   first four; `r152` records that `tsc` accepts the closure form
   and that this rule is narrower.
2. `corpus/accept/a147-switch-body-scope.ts` + `.expected`: a
   `switch` whose cases declare and use distinct names, and one case
   that declares a name a later case does not read, so the one-scope
   rule is exercised without a rejection.
3. **No existing accept entry may move.** If rule 4 rejects one, the
   round stops and reports it: that is evidence the rule is too
   broad, not a golden to update.
4. `corpus/reject/r153-switch-cross-case-write.ts`: a plain
   assignment to a name a different case declares. *(Added
   2026-08-26 after the pass A review, which measured that the first
   round accepted the write and made the tiers disagree: the dev
   tier stopped with "internal lowering error: unbound local
   `counter`" and the ship tier ran. `node` refuses the same program
   with a temporal-dead-zone `ReferenceError`; stock `tsc` accepts
   it, so the entry carries a `tsc-clean-standalone` header.)*
5. *(Added 2026-08-26 after the second pass A review.)* One reject
   entry for an ambient-namespace name shadowed by a later
   declaration, and one for a class name, each with the `tsc` codes
   in its header. One accept entry pins rule 1a: a `using` in a case
   that falls through into a second `using`, printing `case0 /
   case1 / dispose:b / dispose:a / end` byte-exact on both tiers.
6. *(Added 2026-08-26 after the third pass A review.)* One reject
   entry for rule 4b: a local that owns a class name, declared
   before the `new`.
7. Counts: rejects 142 → 151; accept `.ts` 145 → 147; `.expected`
   146 → 148; accept source files 147 → 149.

Pass B:

5. `corpus/accept/a149-suspension-state.ts` + `.expected`: two
   `await` expressions in one expression, with prints that pin the
   evaluation order; a capturing lambda created before a suspension
   and called after it, including one that captures a managed value;
   `await` inside a `for...of` body and `yield` inside a `for...of`
   body; and two generators of one yield type, resumed in turn and
   also passed to a function. Byte-exact across dev JIT, ship C-AOT,
   and the golden.
6. Unit tests: the frame layout of a function with a value live
   across a suspension holds a slot for it; the ship tier emits no
   search over generators; a generator handle carries its resume
   address.
7. Counts, restated 2026-08-26 because pass A moved the base twice:
   accept `.ts` 147 → 148; `.expected` 148 → 149; accept source
   files 149 → 150; rejects unmoved at 151.
9. *(Added 2026-08-26 after the fourth pass B review.)*
   `a149-suspension-state` grows to pin every shape the review
   measured: the three rule 1h shapes, the rule 1i declared-type
   shapes on a parameter and on a `FixedArray` element, the rule 1j
   async-method receiver, and the rule 2a nested `await`. The
   counts of item 7 do not move, because the entry already exists.
   One interop test pins rule 7: a foreign call whose later argument
   grows the array of an earlier one, printing `f2=2`. The record
   quotes the dev-tier and the ship-tier output of each shape.
10. *(Added 2026-08-26 after the fifth pass B review.)*
   `a149-suspension-state` grows again: the four rule 1k shapes, the
   rule 1l unreachable statement after a `return` and after a
   `break`, and the two rule 7a foreign twins as an interop test
   pair. The counts of item 7 do not move. Rule 1k removes code, so
   the record states the frame size of a coroutine that holds one
   capturing lambda, before and after, and states that no golden
   moved.

Both passes:

8. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. Every pre-existing
   golden and `.expected` byte-identical, the new entries excepted.
   The record quotes the test count and the wall time.

## 68. One ordered IR between the checker and the two tiers

Origin: the owner asked on 2026-08-26 why recent fixes need many
review rounds. An audit of the §66 and §67 arcs answers it. This is
not a downstream request. **No language surface moves.** The
accepted TypeScript subset, the C ABI, the host API, the CLI, and
every committed `.expected` stay as they are.

This section opens after §67 lands. Pass A landed at `1c578f9`.
Pass B landed at `9bde577` and is **not COMPLETE**: one CRITICAL
stays open, and it is item 2's `a152` below. §68 closes it. §67 closes instances of the
defect classes below; this section closes the classes.

Measurements re-taken at `9bde577`, where §67 pass B is landed and
not COMPLETE. `codegen/src/` holds 30352 lines. Every measurement is structural. Each one is read
from the committed tree, or quoted from the §66 and §67 records.

1. **The round count separates by area, not by difficulty.** A
   request that changes the checker or the standard library lands in
   one implementation commit: R33 `49bdd1d`, R34 `ca5cb4e`, R36
   `1438b76`, R37 `f29c4c5`. A request that changes codegen
   internals does not. §66 needed seven review rounds. §67 pass A
   needed four rounds and three reviews. §67 pass B needed five
   rounds and five reviews, and it leaves three MINOR and two
   adjacent defects open.
2. **Three traversals of one HIR tree each re-derive the evaluation
   order.** `hir::ExprKind` holds `Box<Expr>` operands
   (`compiler/src/hir.rs`), so no evaluation order exists in the
   data. `codegen/src/lower/func.rs` (9184 lines) walks the tree
   for the dev tier. `codegen/src/cemit.rs` (9763 lines) walks it
   for the ship tier. `codegen/src/suspension.rs` (871 lines) walks
   it for the spill plan. Each walk fixes the order by its own
   convention, and the three conventions must agree.
3. **The same walk exists twice.** `codegen/src/lower/func.rs` and
   `codegen/src/cemit.rs` each declare their own `walk_lets`, their
   own `count_yields`, and their own `count_async_calls`. The two
   `count_yields` must return one number, or the resume label tables
   of the two tiers disagree.
4. **Four §67 pass B CRITICAL findings are one class: traversal N
   disagrees with traversal M.** The round 2 review measured the
   `for` statement. The planner visits it as init, cond, step, body.
   Both tiers lower it as init, cond, body, step. Rule 1h records
   that the liveness scan read HIR source order as evaluation
   order. Rule 1i
   records that the planner took a spill kind from the expression
   type and the dev tier took it from the declared type. Rule 1j
   records a site the planner reserved for and neither tier closed.
5. **The strict cursor reports the disagreement at the user's
   program, not at the build.** The §67 record states it: the cursor
   "did its job — but it caught the defect at the user's program,
   not at the build." Rules 1d, 1e, 1f, 1g, 1h, 1i, 1j, 1k, and 1l
   exist to hold three traversals in step.
6. **Both tiers already want a control-flow graph.** The dev tier
   builds Cranelift blocks. The ship tier emits no structured C:
   every loop and every branch is a label and a `goto`. Measured
   over the whole of `codegen/src/cemit.rs`: **no emitted string
   holds a C `for`, `while`, or `do` keyword.** `emit_while`,
   `emit_for`, `emit_for_of`, and `emit_switch` emit labels and
   `goto` only.
   Both tiers build a graph from a tree, separately, for every
   function, and neither keeps the graph.
7. **The differential gate is blind to a shared wrong answer.**
   Three recorded instances: the `using` disposal order in a
   `switch` case (pass A second review); a second lambda that reuses
   a first lambda's frame member (round 3 MAJOR); the rule 7a
   foreign call that marshals an array count after a later argument
   grows the array (fifth review). The §67 record states the
   consequence each time: "Both tiers agree, so the differential
   gate does not see it."
8. **Two defects of that class stay open, and neither program
   suspends.** The §67 record names them as adjacent defects. A
   `@CStruct` receiver address, taken before an argument grows the
   same array, prints `sync=5` on both tiers, where `sync=12` is
   correct. A second program assigns a lambda inside a loop body,
   and calls it after the loop. The dev tier prints `v=22`. The
   ship tier reads an abandoned C block scope. The §67 answers are
   frame-scoped, and neither program holds a frame.
9. **The ship tier derives C identifiers from source names; the dev
   tier does not.** §66 measurement 5 records it:
   `codegen/src/lower/mod.rs` names a method by index, so every
   divergence of that class is one-sided. §66 closed the class with
   a `v_` prefix, a keyword list, and `_N` collision logic. A prefix
   is a convention that every future emitter site must obey.

Consequence: the two tiers hold the same semantic decisions twice,
and a third copy holds the evaluation order. A review finds one
instance for each round, because the class has no single site.

Rule 1g proved the alternative. When the check became total, the
round named all three remaining sites with their counts — the
number that three reviews did not produce. This section makes
that mechanism the default: **a defect class closes with a total
check at the build, not with a corpus entry for each instance.**

### 68.0 What does not move

The interface is not the subject. This section moves no part of it.

- Every program in `corpus/accept/` compiles and prints the same
  bytes, with the exceptions §68.6 item 2 names and pins as Red.
- The C ABI, the emitted header, the `subscript_*` symbol
  convention, and the host API do not move.
  - *(One addition, 2026-08-26, recorded rather than discovered.)*
    Step 2 added `subscript_rt_trap_index_out_of_bounds`. The dev
    tier is Cranelift and cannot format the bounds message the way
    emitted C does with `snprintf`, so it needs a runtime entry that
    takes the index and the length. Nothing existing changed and no
    host loses what it depends on. **Both tiers call it**, so the
    message has one source instead of two — one instance of the
    duplication this section exists to remove, closed early.
- `specs/blocks/collisions.md` does not move. No collision is
  decided or re-decided here.
- Invariant 3 holds: two execution forms, dev JIT and ship C AOT.
  They keep separate final lowerings. This section moves the shared
  part **above** them, not between them.

### 68.1 The form

1. **One IR sits between the checker and both tiers.** The name is
   LIR. One lowering builds it from typed HIR. Both tiers consume
   it. **No tier reads HIR.**
2. **The evaluation order is data.** Every operand of a LIR
   instruction is a value name or a constant. No operand holds a
   nested expression. The HIR → LIR lowering fixes the order once,
   and writes it as a sequence.
3. **The control flow is a graph.** A LIR function is a list of
   basic blocks. Each block ends with one terminator: branch,
   conditional branch, switch, return, trap, or suspend. No
   structured statement form survives into LIR.
4. **Every value has a name, a type, and exactly one definition.** A
   temporary is a value like any other value.
5. **Every entity carries an id.** A class, a method, a function, a
   local, a block, and a value each carry one id. **A target
   identifier derives from an id.** No target identifier derives
   from a source string. A source name rides beside the id as an
   attribute, for diagnostics and for the reload key. No consumer
   parses that attribute. *(R37 verified the same property for
   method names before its contract: no consumer of a method name
   parses it.)*

6. **A source binding is a value, not storage.** *(Added
   2026-08-26 after the step 1 review, which measured that item 4
   alone does not give this.)* `Local` storage exists only for a
   binding whose address the program takes, and the lowering states
   which those are. Item 4 says that every value has one definition.
   A lowering satisfies item 4 and still routes every binding
   through a function-scope slot, because the loads and the stores
   are then the only values. Measured on the step 1 lowering: 6742
   of 16715 instructions were local traffic, 2399 locals existed
   against 92 block parameters, and 24 of 48 coroutine functions
   read a local after their resume block. That is the storage model
   of §67, which items 6 to 8 of §68.2 exist to retire, so the form
   must forbid it and not only the rules.
7. **Loop traversal state is values.** *(Added 2026-08-26 after the
   step 1 review.)* A cursor, an index, and a bound cross a back
   edge as block parameters. An instruction that advances a
   traversal produces the advanced state as a result. Measured on
   the step 1 lowering: `for...of` created a cursor, stored it in a
   local, and read it in the body and in the condition, and no
   instruction ever wrote it back. Under item 4 the cursor cannot
   change, so the loop yields element 0 for ever, and a tier can
   only run it by holding an index that LIR does not carry.

8. **LIR carries every trap site, once, on the instruction or the
   terminator that owns it, and each carries its position.**
   *(Widened 2026-08-26 after step 2. The rule said "instruction",
   and a terminator owns sites too. `Suspend` lacked the position a
   reload's `StaleCoroutine` reports, and `Return` lacked the one a
   boundary-pointer scratch allocation reports. Two rounds, one
   class, so CLAUDE.md's two-round rule applies and the rule widens
   rather than a third terminator being fixed.)* *(Added 2026-08-26 after the second step 1 review.)*
   A trap site belongs to the operation whose operands the check
   reads. Every HIR node the lowering evaluates contributes its own
   sites, a node reached as the base of a place included. A
   function-level site is carried too: both tiers read the
   coroutine creator's allocation site and refuse the program when
   it is absent. Measured on the second step 1 attempt: `a[idx()].x
   = 9` traps today with "index 5 out of bounds for array length 2",
   and its whole LIR module carried no index trap; 13 entries lost
   the coroutine `Allocation` site, which no part of LIR named; and
   one `DivisionByZero` of `a76` became three, two of them on an
   address computation that has no divisor. A `checked` flag with no
   site and no position is not a trap site — a tier that reads it
   decides semantics, which §68.2 item 10 forbids.
9. **No instruction restates a fact the values table carries.**
   *(Added 2026-08-26 after the second step 1 review.)* An operand's
   type is a property of the value. An instruction that records its
   operand types again gives the verifier two copies of one fact,
   and a check that compares them cannot fire on anything the
   lowering built. Measured: after item 11 was amended, one check
   consulted a declared signature and about ten kept the
   self-comparing shape, including the exact line the first review
   named. Delete the restatement. Derive what an operation requires
   from the operation, and compare that against the values table.

10. **The module names its entry and its async roots by id.**
   *(Added 2026-08-26 after step 2 stopped.)* A consumer runs a
   program, so it needs to know which function starts it, and §26.3's
   standard runner needs every exported zero-parameter async
   function. Neither is derivable from the form today, and the
   interpreter compensated by matching a `source_name` against
   `"main"` — which item 5 forbids, and which this session did not
   catch when it accepted the interpreter. The module carries the
   entry `FunctionId` and the ordered list of async root
   `FunctionId`s.

### 68.2 The rules the form makes true

6. **Liveness is one fixed-point over the graph.** The analysis
   reaches a fixed point across back edges. No other consumer
   computes liveness. This retires §67.2 rules 1b, 1d, 1e, 1f, and
   1h, whose subject is the agreement of two liveness walks.
7. **A suspension is a terminator.** The values live across a
   suspension are the live-in set of its successor block. The frame
   holds that set, and holds nothing else. **No event list exists,
   and no cursor exists.** This retires §67.2 rules 1, 1c, 1g, 1i,
   1j, 1k, and 1l. It also retires rule 2a: a suspension carries a
   block id, so no emitter allocates a label number by hand.
7a. **A `Local` that is live across a suspension lives in the frame,
   and LIR says so.** *(Added 2026-08-28 after the Fable phase review
   of §68 consumers, C1.)* Item 7 says the frame holds the successor's
   live-in set and nothing else. A `Local` is storage, not a value, so
   it was never in that set, and both transcribers gave it a C local
   or a stack slot of the resume function, re-created at every resume.
   Measured at `2a65724` and at `e598994`:

       function* g(): Generator<i32> {
         const fixed: FixedArray<i32, 2> = [1, 2];
         yield fixed[0]; yield fixed[1]; yield fixed[0] + fixed[1];
       }
       dev 1,0,0   ship 1,0,0   interpreter 1,2,3

   `fixed[0] + (await val(3)) + fixed[1]` prints `4` on both tiers and
   `6` on the interpreter. Both tiers agree, so the differential gate
   cannot see it; the interpreter is right.

   The form carries the fact. Every `Local` declares its storage
   class: **activation** (dies with the activation) or **frame**
   (lives in the coroutine frame from its first definition to the
   function's end). The lowering marks a `Local` `frame` when any
   suspension lies between a definition and a use of it. The verifier
   fails a function in which an activation `Local` is read after a
   suspension that a definition of it dominates. Both transcribers
   read the class and decide nothing. Item 7's "nothing else" now
   reads: the frame holds the live-in set and the frame-class locals.

   Corpus: `a164` (a generator and an async function, each with a
   `FixedArray` local and a `FixedArray<CStruct, N>` local read after
   a suspension, in a loop and outside one). Red at `2a65724`.
8. **Storage scope is the live range, never the source block.** If a
   value outlives its source block, the value lives in
   function-scope storage. This closes measurement 8's second
   defect, which holds no frame, by the same rule that serves a
   coroutine.

8a. **A capturing lambda's environment is one instance per execution
   of the literal, in a function-scoped arena.** *(Owner decision
   2026-08-27. Rule 8 alone does not give this, and §68.6 item 2
   named rule 8 as the fix for `a151` and `a152`. That was wrong.)*

   Rule 8 is about **scope**. The defect is about **instance
   count**. One shared function-scope environment closes `a151` by
   accident — the loop's last iteration wins and the one local holds
   the last closure — and leaves `a152` wrong, because `a152` keeps
   iteration 0's closure in a second local while the literal runs
   again. Both tiers print `async-keep=30`, and `10` is correct.

   Three facts fix the shape of the answer:

   - The checker rejects a capturing lambda that escapes its
     defining function: "A capturing lambda may not escape its
     defining function." **The lifetime is bounded by the function's
     activation**, so no collectable allocation is needed.
   - S009 rejects a capturing lambda stored in an array, a field, or
     a global, so a local is its only home.
   - A `SubFn` copy copies the environment **pointer**, so two
     locals alias one environment. A slot per destination is
     therefore not enough either.

   **The reasoning above concluded a bump arena, and that was
   wrong.** *(Corrected 2026-08-27, the same day.)* It assumed the
   `SubFn` copy keeps sharing one environment, so instances must be
   per execution and their count is a loop's trip count. A fourth
   fact removes that assumption:

   - S009: "capturing lambdas may capture only const locals **by
     value**". A capture is immutable and copied, so **sharing an
     environment and copying it are not distinguishable**. No
     program can observe the difference, because no lambda can write
     a captured variable.

   So the environment travels **with the value**: every function-
   typed LIR value owns its environment storage, and a `Copy`, a
   block parameter, an edge, a parameter, and a resume all copy the
   environment rather than alias it. `keep = f` takes iteration 0's
   contents, and a later iteration overwrites `f`'s storage and not
   `keep`'s. The count is then the number of LIR values, which is
   static, and no arena is needed.

   Ordinary functions hold that storage in a shadow frame; a
   coroutine holds it in its own frame, which replaces §67 rule 1k's
   one member per literal — the member `a152` overwrites.

   A closure that does not outlive its block keeps a stack slot. The
   liveness of §68.2 item 6 already answers which is which, and no
   second analysis decides it.

   **What this does not do.** It is not a `Context` allocation. A
   `Context` allocation would hold the environment until an explicit
   collect, against a stack slot that costs nothing today, and it
   would give the user an allocation they cannot `delete`. Invariant
   2 is satisfied by the arena being explicit, scoped, and
   deterministic, not by a collector.

   **Measure, do not assert.** The record states the cost of the
   per-value environment storage. `a22` measured 1.34× with it, the
   same as without, so the cost does not reach the performance gate.

8b. **A value whose address is taken stays rooted for the rest of
   the activation.** *(Added 2026-08-28 after the Fable phase review
   of the post-§70 arc, finding C1.)* `26403be` gave
   `root_storage.rs` a fixed point that follows an address through
   SSA edges, locals, and `StoreAddress` into owners it knows, and
   keeps the base rooted while the address is reachable that way. It
   has no arm for `StoreGlobal`, and a `Call` transfers the
   dependency only into the call's result, so a callee that stores
   the operand ends the chain. Measured at `2a65724`, identical bytes
   on both tiers, expected `<sum>:<len>:1:1:31:47` on every line:

       cond-in-setup=0:0:0:0:8015:8016
       pushcond-in-function=0:0:0:0:8015:8016
       fieldcond-after=0:0:0:0:8015:8016

   One plan feeds both transcribers, so the differential gate cannot
   see it. This is the second instance of the address-base class
   (§33.4 records the first), and the two-round rule applies: the
   form changes, and no arm is added.

   **The rule.** If any instruction takes a value's address, that
   value's root slot is held from the address-taking instruction to
   the activation's end. The plan does not follow the address. It
   does not need to know where the address goes, so there is no arm
   to miss. The fixed point that followed addresses is deleted.

   Cost: one slot per address-taken value, held to function exit. An
   address is taken by a value-class-to-nullable conversion and by
   nothing else a script can write, so the count is small and the
   cost is bounded by the number of such conversions in a function.

   With S015 (§33.4) rejecting every store of such a value into a
   location that outlives the activation, the legal residue is the
   value that lives and dies in one activation, and this rule makes
   that residue sound without a chain.

   Corpus: `a163` — the in-activation shapes the review measured
   that S015 leaves legal, each read back through the foreign
   checker. Red at `2a65724`.
8c. **The form's own invariants are verified, not stated.** *(Added
   2026-08-28 after the Fable phase review of §68 form, C2, M3, M4,
   M5.)* Four checks the section claims did not exist, and each was
   built as a hand-written LIR module and accepted:

   - **Rule 7 / §68.7.4.** A value read after a resume that is not a
     successor parameter was accepted (`b1: %1 = Copy(%0)` after a
     `Suspend` whose successor has no parameters). The verifier treats
     a suspend edge as an ordinary edge. It must not: after a
     `Suspend`, the successor's live-in set is exactly its parameters,
     and a read of any other value fails verification.
   - **`array_base`.** An `Address` whose `array_base` names an
     undeclared value was accepted. The base must be a declared value
     that dominates the address, or verification fails. Every check
     keyed on the base — invalidation, the interpreter's poison
     registry — is silent for a wrong base, so this is the check that
     guards the others.
   - **Item 11 for intrinsics and built-in methods.** The verifier
     compared a call's operands against the `parameter_types` on the
     same instruction, so a `Math.Abs` called with three strings and a
     self-agreeing record verified clean. That is the shape core
     principle 9 forbids. **The module carries one signature table for
     intrinsics and built-in methods, derived from the checker's
     definitions, and the verifier compares every such call against
     it.** `CallTarget.parameter_types` for those kinds is then the
     restatement item 9 forbids, and it goes.
   - **Item 12's exhaustiveness.** The fact check enumerates from
     HIR's types without a wildcard for `TrapSite` only, because
     `ExprKind`, `Stmt`, `Callee`, and `AsyncCallee` are
     `#[non_exhaustive]` and force a wildcard in another crate. A new
     suspending or calling kind is then silently unchecked. **Those
     four enums lose `#[non_exhaustive]`**, as `TrapSite` did. CLAUDE.md's
     convention is for public extensible enums; HIR is this project's
     internal form, and a consumer that must be total over it is the
     point. The check then names every new kind at compile time.

   Item 12's "fails the build" is corrected to "fails the suite": the
   fact check runs in the test suite over every corpus entry, and the
   CLI does not run it. Totality over facts is the property; when it
   runs is not.

   The interpreter's exclusion list is part of the form's record. An
   exclusion whose reason no longer holds is removed: `a153` runs and
   matches its golden at `2a65724`, and the list said it could not.
9. **An address is a value, and it carries an invalidation point.**
   Every LIR instruction that can move an array's storage names the
   arrays that it invalidates. The lowering re-computes an address
   that crosses an invalidation of its base. This closes §67 rule 7,
   the rule 7a conflict, and measurement 8's first defect, as one
   rule instead of three sites.
10. **Neither tier decides semantics.** Each tier is a total
    function from LIR to its target. If a tier needs a fact that LIR
    does not carry, LIR is wrong. The round reports it and stops.
    That report is the intended outcome, not a failure of the round.
11. **A verifier runs on every LIR function, in every build.** It
    checks that every use is dominated by its definition; that every
    value has one definition; that every block ends with one
    terminator; that no address crosses an invalidation of its base;
    and that every operand type matches its instruction. **Every
    check compares two things the lowering derived separately.** A
    check that compares a record against the expression that built
    it cannot fire. Measured on the step 1 verifier: the call check
    read `instruction.operand_types != target.parameter_types`, and
    both sides came from one `map` over one operand list, so a call
    that passed three wrong operands to a one-parameter function
    verified clean. A call compares against the **callee's declared
    signature**. An intrinsic operation compares against a table
    that LIR carries, not against a positional index into a Rust
    array. The verifier's own tests must build the violating form,
    not mutate the record that the check reads. The
    verifier runs in the debug profile and in the release profile.
    This is rule 1g, generalized from spill slots to the whole form.

12. **A total check reports every fact that LIR drops.** *(Added
    2026-08-26. Two reviews raised one class, so CLAUDE.md's
    two-round rule applies: the form changes, and no third instance
    is fixed by hand.)* The first step 1 review found that a
    `for...of` needed an index and a bound that LIR did not carry.
    The second found that no part of LIR named the coroutine
    creator's allocation trap site, which both tiers read and
    require, and that 13 entries lost it. Each was found by reading.
    The build now compares, for every corpus entry, each fact a tier
    reads out of HIR against what LIR carries: a trap site per
    expression and per function, the entity ids, and the operand
    counts. A dropped fact fails the build and names the entry and
    the position. This is rule 1g of §67 in its general form: a
    total check turns a review's search into a build's list.

    **The check is only as complete as what we know a consumer
    needs, and writing a consumer is what tests it.** *(Added
    2026-08-26.)* The first run reported 153 dropped facts — every
    entry id and every async root — in one list. Step 2 then found a
    fact the check did not know to look for: a `Suspend`'s position,
    which a resume after a reload reports with `StaleCoroutine`.
    Adding that item made the check report 49 sites at once. So the
    relationship is the same one §68.7.5 states for the section: the
    interpreter tests §68.7, and each new consumer tests this check.
    A consumer that finds a dropped fact reports a defect in the
    check as well as in LIR. The check verifies a position on every
    terminator that owns a trap site, not on a named list of them.

    **The check enumerates its fact kinds from HIR's own types,
    exhaustively.** *(Added 2026-08-27 after step 3. Two rounds found
    a kind the check did not enumerate: a module with no entry, where
    the check compared presence and not absence; and a host entry's
    parameter validation, an attachment point beside the expression
    and the function that HIR carries ad hoc. CLAUDE.md's two-round
    rule applies, so the rule changes rather than a third kind being
    added by hand.)* A fact kind that HIR gains and the check does not
    learn is a compile error, not a silent omission. The check
    compares both directions: a fact HIR has and LIR drops, and a
    fact LIR has and HIR does not.

### 68.3 What retires

The deletions are part of the contract. §68.6 item 5 measures
them.

- `codegen/src/suspension.rs`, in whole: `SpillPlan`, `SpillEvent`,
  `EvalEvent`, the trace builder, the strict cursor, and the
  exhaustion check.
- §67.2 rules 1, 1b through 1l, 2a, 7, and 7a, as hand-written
  sites. Each rule keeps its corpus entry. No rule keeps a site.
  §67.1 is checker semantics and does not move.
- The duplicate walks of measurement 3. One lowering replaces both
  copies of each walk.
- §66's `v_` prefix, its keyword list, and its `_N` collision logic.
  A C identifier derives from a LIR id, so no source name reaches
  the C namespace.
- The per-tier dominance and ordering assertions that each tier
  holds today.

### 68.4 The order of the work

The differential gate guards this migration, if one tier moves at a
time. Each step ends with the full gate of §68.6 item 7. If a step
moves a committed golden, the step stops and reports it.

1. Define LIR. Write the HIR → LIR lowering and the §68.2 item 11
   verifier. Neither tier changes yet. The verifier runs over every
   corpus entry.
   **Nothing consumes LIR at this step, so no gate tests it.** The
   verifier is the only check, and one round writes both. The step
   therefore ends with a review that builds violating LIR by hand
   for each check of item 11, and that reads the LIR of named corpus
   entries against their known behaviour. *(Added 2026-08-26. The
   first attempt at this step passed every gate with a lowering that
   could not terminate a `for...of` and a call check that could not
   fire.)*
1b. **Write a reference interpreter for LIR.** *(Owner,
   2026-08-26. Inserted before step 2.)* It runs every corpus entry
   and its output joins the standing gate: interpreter ≡ dev ≡ ship
   ≡ golden. Neither tier changes yet.

   The reason: step 1 has no gate that tests LIR, because nothing
   consumes LIR. Both step 1 reviews found every CRITICAL by
   reading, and each cost an hour. Each one shows as a wrong output
   the moment LIR runs — a `for...of` cursor that never advances
   yields element 0 for ever; a dropped trap site does not trap; a
   binding read out of storage after a resume reads what the resume
   abandoned.

   It also gives step 2 a tiebreaker. When a dev tier on LIR
   disagrees with a ship tier on HIR, the interpreter says which
   side moved.

   **The tiebreaker has a blind spot, measured at step 2.** The
   interpreter's declared exclusions are almost all interop entries,
   because they need a native library it does not load. Step 2's
   dev-and-ship disagreements were `a97`, `a124`, and `a125` — all
   interop entries. So the third witness was unavailable exactly
   where the disagreement was, and the round reported "cannot
   adjudicate" rather than name a side. That is the right report,
   and it bounds what this step's tiebreaker is worth.

   **Boundaries.** The interpreter is not a third execution form,
   and invariant 3 does not move: the two shipped forms stay dev JIT
   and ship C AOT. The interpreter is a test oracle and is never
   shipped. It does not replace the golden; it agrees with it, as
   the tiers do. It links `runtime/`, so it shares the runtime and
   finds no runtime defect — it finds lowering and LIR defects,
   which is where every defect of §66, §67, and §68 step 1 lived.
   An entry the interpreter cannot run is named in a declared list
   with its reason. A silent skip is an escape hatch.

   It answers principle 12 as well: written from this section rather
   than from either tier, it does not share a tier's assumption. The
   two open defects that both tiers agree on are exactly that shape.
2. Move the dev tier to LIR. The ship tier stays on HIR. The
   differential gate now compares one LIR consumer against one HIR
   consumer, so it guards this step directly.
3. Move the ship tier to LIR. `cemit.rs` becomes a transcriber of
   blocks and instructions.
4. Delete `codegen/src/suspension.rs` and both cursors. Rules 6 and
   7 of §68.2 now carry what the cursors carried.
5. Move C identifiers to id form. Delete the `v_` prefix.

Steps 2 and 3 are the two steps that carry risk. Step 2 keeps a
working ship tier as the reference. Step 3 keeps a working dev tier
as the reference.

### 68.5 Changes by site

`compiler/`: `hir` is unchanged. The checker is unchanged. A new
module holds the LIR types.

`codegen/`: a new module holds the HIR → LIR lowering and the
verifier. `lower/func.rs` becomes a LIR → Cranelift transcriber.
`cemit.rs` becomes a LIR → C transcriber. `suspension.rs` is
deleted. `lower/mod.rs` keeps the symbol tables, and takes the C
name construction that `cemit.rs` holds today.

*(Recorded 2026-08-27 after step 3.)* `lower/mod.rs` **decides no
semantics**, which is item 10 applied to it. After step 2 it still
held 64 `hir::` references, and one of them derived a host entry's
wire-alias validation from HIR. That is why `t50` passed on the dev
tier and failed on the ship tier: one consumer had a fact the form
did not carry. The symbol tables and the C names are its whole role.
Step 2's commit says "the dev tier reads LIR"; `lower/func.rs` does,
and `lower/mod.rs` did not.

`runtime/`: unchanged. No runtime entry point moves.

### 68.6 Corpus and gate (pre-registered exit criteria)

1. **No committed golden or `.expected` moves**, except the entries
   item 2 names. A golden that moves is evidence of a defect in the
   step, not a golden to update. The round stops and reports it.
2. **The two open defects of measurement 8 close, and neither
   closes with a site-specific fix.** This is the sharpest test of
   the form. If either defect needs a hand-written site in a tier,
   LIR is wrong, and the round reports that instead of the fix.
   - `corpus/accept/a150-receiver-address-invalidation`: a
     `@CStruct` value class in an array, called as a method
     receiver, with an argument that grows the same array. Red at
     the contract pin: both tiers print `sync=5`, and `sync=12` is
     correct. The control line `ctl=12` stays correct at the pin.
   - `corpus/accept/a151-lambda-env-outlives-block`: a lambda
     assigned inside a loop body and called after the loop, with no
     coroutine. Red at the contract pin: the dev tier prints
     `v=22`, which is correct, and the ship tier printed `v=-1`.
     The ship tier reads an abandoned C block scope, so its value
     varies between runs.
   - `corpus/accept/a152-lambda-env-per-iteration`: the coroutine
     twin of the entry above. A lambda literal inside a loop body
     in an async function, held past the loop, with a suspension in
     the body. Red at the contract pin: both tiers print
     `async-keep=30`, where `async-keep=10` is correct. *(Added
     2026-08-26. The §67 pass B sixth review found it. Before §67
     round 6 the dev tier printed `async-keep=-2083027712` and the
     ship tier printed `async-keep=0`, so the tiers disagreed and
     the differential gate saw it. Round 6 made them agree on the
     wrong answer, which the gate cannot see. One frame member serves one lambda literal, and a literal
     inside a loop runs many times. §68.2 rule 8 is the fix: the
     storage scope is the live range, never the source block. A
     narrowing patch in §67 would be the seventh of its kind, so
     the defect moves here whole.)*
   - Every entry must be **Red at the contract pin, verified
     against a binary built from that pin.** *(The §67 lesson, in
     one line: a corpus entry that never failed before the fix
     proves nothing.)*
3. **LIR text goldens** for a named subset: every entry that holds a
   coroutine, plus the §66 and §67 measurement entries (`a145`,
   `a147`, `a148`, `a149`, `a150`, `a151`). The text form makes a
   LIR change reviewable as a diff. The rest of the corpus is
   covered by the verifier and by the existing goldens.
4. **Performance.** §3 fixes ship-AOT at 1.5× of the C baseline and
   dev-JIT at 4×. §11's bisection records 1.53× post-P19, with the
   trap checks that C6 requires. LIR names many temporaries, and the
   emitted C depends on the C compiler to coalesce them, so this is
   the named risk of the whole section. The round measures
   `a22-matrix-propagation` by the §9 methodology, before and after,
   on one machine in one session.
   - **`a22` alone was not enough, measured 2026-08-27.** This item
     gates one entry, and `a22` is matrix propagation: it allocates
     almost nothing. The `collect` workload of the cross-language
     suite — 20000 nodes over 6 rounds, each owning strings, 15000
     kept and the rest freed — regressed through §68 and no gate
     saw it. Bisected on one machine in one session, with the C
     baseline steady at 32.9 to 33.9 ms:

         pin            ship             dev-JIT
         9bde577      211.7 ms 6.43x   229.3 ms 6.97x   before §68
         628a491      273.5 ms 8.07x   344.5 ms 10.17x  after §68
         662a9ec      274.0 ms 8.11x   342.7 ms 10.14x  now

     §70 is not the cause: `628a491` is the commit before it and
     already carries the regression. LuaJIT and V8 measured the same
     across the pins, so the machine is not the cause either.

     **The allocation and free path has no standing gate.** The
     cross-language suite holds the workloads that exercise it, it
     runs by hand, and nothing ran it between 2026-07-27 and
     2026-08-27. The owner asking for an updated benchmark table is
     what found this.

     **The cause, measured: a dead LIR temporary stays a GC root for
     the whole activation.** Runtime call counts are identical at
     every pin, so it is not extra calls. The time is inside
     `Context.collect()`: 123 ms before §68, 297 ms at `8084c45`,
     189 ms at `628a491`. Live allocations say why — the collector
     reached 75005 per round and 5 at the end before §68, and leaves
     100006 to 175006 per round and 100006 at the end after it. The
     surplus is exact: 25000 allocations is 5000 dropped nodes times
     a node and its four strings, and 4720000 bytes is 5000 times
     944 bytes of that group's ship-tier capacity. A stale root holds
     the head of the chain the program deliberately dropped.

     **This violates §68.2 rule 8**, which says storage scope is the
     live range and never the source block. Rooting a value for the
     whole activation is precisely what that rule forbids, so this is
     a defect and not a trade this section made.

     **Closed 2026-08-28.** A shared root-storage plan derives slot
     interference from the liveness §68.2 item 6 already computes —
     no second fixed point — reuses a slot only across non-
     overlapping live ranges, and clears a slot when its value dies.
     Both transcribers consume that one plan.

         collect          before §68    regressed     fixed
         ship             211.7 ms      274.0 ms      207.5 ms
         dev-JIT          229.3 ms      342.7 ms      227.5 ms
         live per round   75005         100006+       75005
         live at the end  5             100006        5

     The live counts are the mechanism closed, not the timing
     improved: the 5000-node chain the program dropped is freed
     again, and both tiers agree exactly.

     **The cost, recorded.** `tree` — thirty depth-16 trees built,
     traversed, and freed with explicit `Context.free` — moved on the
     ship tier from 1.51× to 1.67×, measured twice on a quiet
     machine. Clearing a slot when its value dies costs a write, and
     `tree` frees densely. Its dev tier improved from 7.81× to 6.22×.
     **The trade is accepted**: a program that calls `collect()` and
     does not reclaim is worse than one that reclaims and runs 17 per
     cent slower on one allocation-dense shape. Invariant 2 says a
     program that never collects is correct and merely larger; it
     does not say a program that collects may keep the garbage.

     **One defect was hiding another.** The dead temporaries also
     masked a missing dev-JIT managed-global root registration, which
     surfaced and was fixed only once they were cleared.

   - **Kill criterion: a ship-AOT ratio above 1.75× stops the phase
     and reopens the form of the emitted C.**

     **Measured 2026-08-27, and the criterion is tripped.** Before
     and after, one machine, one session, `--warmup 60 --timed 15`,
     every subject's spread inside ±20 per cent, and the C baseline
     the same on both sides, which is what makes the pair valid.

         subject      before `9bde577`   after §68 step 3
         C              3.975 ms 1.00x    3.972 ms 1.00x
         emitted-C      6.099 ms 1.53x   15.928 ms 4.01x
         dev-JIT      114.151 ms 28.72x 154.661 ms 38.98x

     The ship tier is `emitted-C`; the harness's `ship-AOT` row is
     the Cranelift AOT that §11 superseded and is a cross-check.
     `1.53x` before matches §11's post-P19 record exactly, so the
     pin met this criterion and §68 is the cause.

     **The named risk is what happened.** This item predicted that
     LIR names many temporaries and that the emitted C would depend
     on the C compiler to coalesce them. The dominant cost is
     narrower than that: the innermost loop of `multiply` now takes
     the address of the locals that hold its two matrix parameters.

         after    v29 = &l0; v30 = &((v29)->d0);
                  v31 = &(((v30)->a)[v28]); v32 = *(v31);
         before   ((v_left).elements).a[((v_row * 4) + v_inner)]

     Taking `&` of a 64-byte local forces it to memory for the whole
     function, so the before shape could stay in registers and the
     after shape cannot. §68.2 item 9 makes an address a value, and
     the transcriber spells that value literally.

     `multiply` also declares 62 function-scope locals, one per SSA
     value, and copies block parameters at every edge. Both are
     secondary: the label-and-`goto` shape is unchanged from the pin,
     which measured 1.53x with it.

     **What reopens is the emitted C's form, not LIR.** An address
     chain consumed only by a load or a store is a member expression
     in C: `&x`, then `->f`, then `[i]`, then `*` is `x.f.a[i]`. The
     transcriber chooses the spelling; LIR keeps saying "address
     value".

     **Cleared 2026-08-27 at 1.34×, which is better than the pin.**
     The ship tier now measures 5.329 ms against 3.987 ms of hand C,
     with every subject's spread inside ±20 per cent. The pin was
     1.53×, so the migration ends faster than it started.

     **The cause was none of the three this session diagnosed
     first.** Address folding took 4.01× to 4.04×. Coalescing block
     parameters out of SSA took it to 4.00×. Four prologue fixes
     took it to 3.98×. Each was a real improvement to the emitted
     code and none of them mattered.

     The owner asked whether array element access called a foreign
     function more than once at a site, and whether `array_len` was
     called repeatedly. Both were true. Counted in the emitted C for
     `a22`:

         helper                     pin   before the fix
         subscript_rt_array_len       4      22
         subscript_rt_array_ptr      11       0
         subscript_rt_array_data      1      10

     The pin read `((SsArrayHeader*)h)->len` and `->data` inline and
     called the runtime **only inside a failed bounds branch**. The
     transcriber called `subscript_rt_array_len` for the test, again
     to build the trap message, and `subscript_rt_array_data` for
     the pointer — two or three opaque calls per element access, and
     one more per loop iteration for the loop condition.

     **An opaque call in a loop body is why the other three fixes
     did nothing.** The C compiler must assume such a call writes
     memory, so it cannot hoist, cannot vectorise, and spills every
     cached value across it. The aggregate copies were the symptom.
     Reading the header inline removed every one of those calls from
     `a22`'s emitted C and took 3.98× to 1.34×. The dev tier had the
     same shape and the same fix: 38.98× to 30.57×.

     **The lesson, once.** Three diagnoses failed because each read
     the emitted C for what looked wrong rather than for what the
     optimizer could not see through. Measuring one change at a time
     is what proved each of them worthless.
   - A ratio between 1.53× and 1.75× is reported to the owner with
     the emitted C of the inner loop, and the owner decides.
   - A dev-JIT ratio above 4× stops the phase.
5. **Line count.** The round reports the line count of
   `codegen/src/` before and after. This section predicted a
   decrease. An increase is evidence that the split is wrong, and
   the round reports it with the measurement.

   **Measured after step 3, and the prediction did not hold.**
   `codegen/src/` went from 30352 lines to 36195, an increase of
   5843.

       the two consumers   18951 -> 13345   (-5606, 30 per cent)
         lower/func.rs      9184 ->  7085
         cemit.rs           9767 ->  6260
       deleted             suspension.rs 871, trap_sites.rs 102
       added               lir.rs 7095, interpreter.rs 5129

   The consumers shrank as predicted. The lowering, the verifier,
   and the fact check cost about what the consumers saved, so the
   migration is flat, not down. The interpreter is 5129 of the
   increase; it is a test oracle the owner added at step 1b, after
   this item was written, and it is not part of the migration.

   **The prediction's premise was wrong, and the split is not.**
   The premise was that removing duplicate walks dominates. What
   dominates is making implicit knowledge explicit. The three walks
   were short because each re-derived, ad hoc, only what it needed;
   one lowering must state all of it once, and the verifier and the
   fact check had no counterpart at all before this section.

   So this item stops being a pass-or-fail gate and becomes a
   measurement with a reading. Judge the split on the consumers'
   size, which fell 30 per cent, on the duplicate walks being gone —
   `count_yields`, `walk_lets`, and `count_async_calls` existed once
   per tier — and on the count of facts that were implicit and are
   now checked, which is ten.
6. **Build and suite time.** The verifier runs in every build. The
   round records the debug and release suite wall time before and
   after. An increase above 20% goes to the owner with the
   measurement.
7. **Gates**: `cargo test --offline --workspace` in both profiles;
   a zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. The record quotes the
   test count and the wall time for each step of §68.4.
8. **Tracking**: `specs/tracking/s68-one-ordered-ir.md`. Each step
   of §68.4 records its own gate run.

### 68.7 What a LIR instruction means

*(Added 2026-08-26. Step 1b stopped and reported that §68 defines the
form of LIR and not the meaning of its instructions, so no
interpreter can be written from this section. CLAUDE.md principle 8
makes that report the wanted outcome.)*

The finding matters more than the gap. §68.2 item 10 says that
neither tier decides semantics. **While the meaning of an instruction
is undefined, each tier decides it.** The two tiers agree today
because one lowering built both conventions out of HIR, not because
LIR pins a meaning. A differential gate between two tiers cannot see
that difference, which is CLAUDE.md principle 12 one level up.

**The interpreter is the completeness test for this section.** If a
reader writes it from this document alone, the document is complete.
If the reader must consult a tier, the document is not.

#### 68.7.1 How this section defines a meaning

1. **By reference where the source language already decides.** An
   instruction that carries a construct of the language means what
   the section that decides that construct says. This section names
   the section. It does not restate it.
2. **By definition where LIR has no source counterpart.** The
   iteration protocol, the address and provenance model, the
   suspension protocol, and edge argument transfer exist only in
   LIR. §68.7.4 defines them.
3. **Operands are positional.** Each row states the operand roles in
   order. The type of an operand comes from the values table, never
   from a record on the instruction (§68.1 item 9).
4. **A trap fires before the operation's effect**, in the order the
   instruction lists its sites (§68.1 item 8). A trap ends the
   program through the observer of §18.
5. **A gap is reported, never guessed.** If a reader cannot act on a
   row, the row is wrong. Report it and stop.

*(2026-08-28, review of §68 form, C1 and M1.)* Item 4 is the contract,
and the interpreter did not implement it. It read `instruction.traps`
at one place, integer `Div`/`Rem`, and raised every other trap it
raised from the runtime library or a `Trap` terminator. Measured: a
fixed-array read at index 4 of length 2 trapped on both tiers and
printed `1805878962` on the interpreter, then `4` on a second run — a
read past the payload in the oracle. `t01` (`JsonResultValue`) trapped
on both tiers and printed `0` on the interpreter. A trap position it
did raise was the instruction's, not the site's (`t27`: tiers `25:3`,
interpreter `25:19`), and no gate compared columns.

**The interpreter raises every site-owned trap kind from one dispatch
over `instruction.traps`, before the operation's effect, at the site's
position.** It has no per-kind arm and no runtime fallback for a kind
LIR owns. The trap gate compares line and column on every tier and on
the interpreter.

*(2026-08-28, review of §68 consumers, C4 and M1. Two conversions
this section said "as C does" are undefined in C, and each tier
decided one.)*

**A float to integer `as` conversion saturates, and `NaN` converts to
zero.** The value is truncated toward zero; a result below the
target's minimum is the minimum, above its maximum is the maximum;
`NaN` is `0`. Measured at `2a65724`: both tiers already do this
(`1e10 as i32` is `2147483647`, `(-1.0) as u32` is `0`, `300.0 as i8`
is `127`), and the interpreter wrapped (`1410065408`, `4294967295`,
`44`). The interpreter changes. C leaves the out-of-range case
undefined; the emitted C calls a helper that saturates, and the dev
tier uses the saturating convert. JavaScript has no such conversion,
and `as` is a no-op there, so a program that prints such a value is
not comparable: collisions.md C3 names it.

**Float `%` is the C `fmod`**, and it is in the language. The checker
accepted it, the dev tier refused it ("floating remainder is not
supported"), the ship tier emitted C `%` on a `double` and did not
compile, and the interpreter ran `fmod`. `fmod` agrees with
JavaScript's `%` for every IEEE case: the sign of the dividend, `x % 0`
is `NaN`, `Infinity % y` is `NaN`, `x % Infinity` is `x`, `NaN`
propagates. Both tiers call the runtime's `fmod`; Cranelift has no
float remainder. A program that prints such a value is comparable.

Corpus: `a165` (the three saturating conversions above, `NaN as i32`,
and float `%` over the seven cases). Its float `%` half is
`js-comparable: yes`; its conversion half cites C3.

#### 68.7.2 The instruction table

Numerics, string, and array behaviour come from the list at §2 and
from §16 for the narrow widths: two's-complement wrap on
`i32`/`u32`/`i64`/`u64`, `as` conversions that truncate and wrap as C
does, `f32` arithmetic at `f32` precision, true 64-bit bitwise
operations, and §2's Q14 formatting for interpolation.

| instruction | operands | means |
|---|---|---|
| `Copy` | value | the same value, of the same type. A value class copies by value (§62); a reference class copies the handle. |
| `StringLiteral` | none | a string of the module's literal table. §2 string rules. |
| `Zero` | none | the zero of the result type: `0`, `0.0`, `false`, or the null handle. |
| `LoadLocal` | none | the current value of the local. §68.1 item 6 limits a local to a binding whose address the program takes. |
| `StoreLocal` | value | the local takes the value. No result. |
| `AddressOfLocal` | none | the address of the local. §68.7.4 gives the address model. |
| `LoadGlobal`, `StoreGlobal`, `AddressOfGlobal` | as the local forms | the same, on a module global. |
| `FunctionRef` | none | the callable of a declared function, for an indirect call. |
| `MakeClosure` | one per capture, in declaration order | a callable with an environment that holds the captured values. §68.2 item 8 governs where the environment lives. |
| `Unary` | operand | §2 numerics, for the named operator. |
| `Binary` | left, right | §2 numerics, string, and comparison rules, for the named operator. Division by zero traps. |
| `Cast` | value | an explicit `as` conversion. §2: truncate and wrap as C does. §16 for the narrow widths. |
| `Coerce` | value | an implicit widening the checker inserted. It never loses a value. A conversion that loses a value is a `Cast`. |
| `AllocateClass` | none | a new instance, fields at their zero. The `Allocation` trap fires on failure. |
| `AddressOfValue` | value | the address of a temporary that holds the value. The temporary lives as long as the address (§68.2 item 8). |
| `AddressOfField` | base address or handle | the address of the named field. A field of an aggregate **value** has no address; `LoadField` reads it. An address exists only where the base has one. |
| `AddressOfIndex` | base, index | the address of the element. `checked` states that a bounds trap site is present; the site, not the flag, raises it (§68.1 item 8). |
| `LoadAddress` | address | the value at the address. |
| `StoreAddress` | address, value | the value is written at the address. No result. |
| `LoadField` | base handle, or an aggregate value | the field's value. The base is a reference-class handle, or a value that holds an aggregate: a value class, a `FixedArray`, or a built-in aggregate such as the generator's iteration result. *(Corrected 2026-08-26 after step 1b. The row named only the reference-class path, and 23 of the interpreter's 30 findings were the value path.)* |
| `Length` | container | the element count of an array, and the **byte** length of a string (Q5, `stdlib.md` §14). |
| `ArrayLiteral` | one per element, in order | a new array of the elements. |
| `ArraySpreadLiteral` | one per part, in order | a new array; a spread part contributes its elements in order (`stdlib.md` §14). |
| `Template` | one per interpolation, in order | the concatenation, formatted by §2's Q14 rules. |
| `Template` (no parts) | — | *(Added 2026-08-28, review of §68 consumers, C3.)* the empty string, and no trap. The dev tier emitted `""` and the ship tier reported `template consumed 0 of 1 traps`; the two transcribers decided an unstated case. It is stated here, and the lowering attaches no trap site to an empty template. |
| `Call` | see §68.7.3 | a call of the named target. |
| `IteratorCreate`, `IteratorHasNext`, `IteratorValue`, `IteratorBound`, `IteratorAdvance` | see §68.7.4 | the iteration protocol. |
| `ForeignArrayData` | array | *(Added 2026-08-28, review of §68 form, M2.)* the array's current data pointer, as an `Address` whose provenance is the array. A foreign call takes it as an operand for each array argument, so the call carries the snapshot §68.7.3 names. It is invalidated by the same instructions that invalidate any address into that array. |
| `Coerce` (an `Address` to a boundary value class, into that class's `Nullable`) | address | *(Same review, M2.)* the address as a non-null pointer value of the nullable type. This is the one `Coerce` that is not a widening: it converts an address to the C-visible pointer a boundary struct-pointer member holds (§33.1). It is legal only for a boundary value class, and the verifier admits no other address-to-data `Coerce` (§68.2 item 11). Rule 8b keeps the base rooted for the activation, and S015 keeps the value from leaving it. |
| `AllocateClass` (a value class) | — | *(Same review, M2.)* an `Address` of fresh zero-initialized storage for the class, in the activation, not "a new instance". A reference class yields a handle; a value class yields the address its constructor writes through. |
| `AsyncHandleCreate(target)` | the call's operands | *(Added for §70; the rows were missing.)* a new coroutine frame for the target, not polled, with its owner count at one, held by the result. |
| `AsyncHandleRetain` | handle | the same handle; the frame's owner count is one higher. |
| `AsyncHandleRelease` | handle | no value; the frame's owner count is one lower, and at zero the frame is freed at this instruction (§70.3 rule 3). |
| `AsyncHandleArrayRetain` | array of handles | no value; every element's frame is retained once. |
| `AsyncHandleArrayRelease` | array of handles | no value; every element's frame is released once. |

#### 68.7.3 What a call means

The target kind decides the operand roles.

| kind | operands | means |
|---|---|---|
| `Function` | the declared parameters, in order | a call of the module function. |
| `Method` | the receiver first, then the parameters | a call of the class method. A value-class receiver is an address; a reference-class receiver is a handle. |
| `Foreign` | the marshalled arguments, in order | a call across the C ABI. **The call-time view.** *(Owner, 2026-08-28: "call-time view でいきましょう". This replaces the sentence "an array argument's data pointer and count are read before a later argument runs ... taken at the argument's evaluation point", added 2026-08-26.)* An array argument is the array, as JavaScript passes a reference. The call carries the array's data pointer and count as operands, **read after every argument is evaluated and immediately before the call**. A later argument that grows the array is visible to the callee, whether or not it suspends. No snapshot exists that a later argument can invalidate, so rule 9 has nothing to recompute here and no stale pointer can reach the C side. §68.2 item 12's check verifies that the operands are read after the last argument. |
| `Indirect` | the callable first, then the parameters | a call through a value of `Type::Func`. |
| `Intrinsic` | the family's operands, in order | the operation the module's intrinsic table names. The table, not a positional index into a Rust array, defines it (§68.2 item 11). |
| `BuiltinMethod` | the receiver first, then the parameters | the standard-library method. `stdlib.md` decides each one. |

*(2026-08-28, review of §68 consumers, M6 — **open, the owner's**.)*
The sentence above and the tree disagree, and the tree disagrees with
itself. Measured at `2a65724`, a foreign call whose array argument is
grown by a later argument:

    grow in a later argument, no suspension            2   (snapshot before the later argument)
    grow after an await earlier in the function        2
    grow in a later argument that contains an await    3   (snapshot in the resume block, after grow)

`codegen/tests/interop.rs` pins the third as `f2suspend=3`. The
sentence above gives `2` for all three. Rule 9 forces the third: the
snapshot is an address into the array, `grow` invalidates it, and an
address that crosses an invalidation is recomputed. The first case
carries the hazard rule 9 exists for: a snapshot taken before a later
argument that reallocates the array is a stale pointer at the call.

Two consistent answers exist. **Call-time view**: the snapshot is
taken after every argument, so all three print `3`, the array is the
reference JavaScript passes, and no stale pointer is possible.
**Evaluation-point view, made safe**: the snapshot is taken at the
argument's evaluation point and the later argument's growth is a
trap, so the first and third cases stop. The first keeps §67.2 rule 7's
intent for sync code and changes one pinned value; the second changes
which programs run. **Decided: the call-time view** *(Owner, 2026-08-28)*. All three
cases print `3`. Every test pin that recorded the evaluation-point
value moves to the call-time value: in `codegen/tests/interop.rs`,
the `f2sync=2` twin and
`foreign_call_without_suspension_preserves_marshalling_order`'s
`f2=2` both become `3`; the `f2suspend=3` twin is already the
call-time value. No corpus `.expected` prints the sync case (`a149`
does not), so none moves.

#### 68.7.4 The three protocols that LIR alone defines

**Iteration.** `stdlib.md` §14 decides what `for...of` accepts and in
what order. §14.3 states that the loop is an index loop over the
container's own storage and that no iterator object exists. LIR
carries that as five instructions over three values — a cursor, an
index, and a bound — which §68.1 item 7 threads across the back edge.

- `IteratorCreate(kind)` takes the subject and produces the cursor.
- `IteratorBound` takes the cursor and produces the bound. *(Revised
  2026-08-29, owner, C13 retired.)* **Which bound depends on the
  spelling, and LIR carries which.** `Array.prototype.forEach` fixes
  its range before the first call, as ECMA does, so its bound is the
  element count at creation, read once. `for...of` over any kind, and
  `Map`/`Set` `forEach`, observe the live container: their bound is the
  container's current element count, read at every step, so an append
  during the traversal is visited. The lowering states the choice on
  the cursor's kind, both tiers and the interpreter read it, and the
  verifier checks that a fixed-bound cursor is created only by an
  `Array.forEach` lowering. Under the live bound the "position is
  within the container's current element count" clause of
  `IteratorHasNext` is the bound.
- **The cursor names a position in the container's own storage.** The
  bound is a position too, captured at creation. A kind whose storage
  holds no hole — an array, a `FixedArray`, a string — has the
  position equal to the index.
- `IteratorHasNext` takes cursor, index, and bound. It is true while
  the cursor names a live position, that position is below the bound,
  and the position is within the container's current element count.
- `IteratorValue` takes cursor, index, and bound, and produces the
  entry at the cursor's position, as `stdlib.md` §14 names it for the
  kind.
- `IteratorAdvance` takes cursor, index, and bound, and produces **one
  result: the next cursor**, at the next live position after the
  current one. The index advances by one, separately, and no rule
  reads the index for liveness.

*(Corrected 2026-08-26, twice, after step 1b. The first text ended a
traversal on the bound alone and trapped on `a80`. The second gave
`IteratorAdvance` two results, which no LIR instruction has. The
cursor carries the position, so one result is enough.)*

`corpus/accept/a80-for-of-foreach-mutation` decides the rule, in its
own header: "appends do not extend and removals shorten". The bound
is captured at a position, so an entry appended past it is never
reached. The current count is read, so a removal ends the traversal
early. An inactive position is never a body iteration, and the
protocol needs no edge that skips the body.

Worked against a80's golden. A `Map` of keys 1, 2, 3 deletes key 2
and appends key 4 on the first step, and the bound is 3:

    cursor at position 0, live, 0 < 3       visits key 1
    advance skips dead position 1           cursor at position 2
    cursor at position 2, live, 2 < 3       visits key 3
    advance                                 cursor at position 3
    position 3 is not below the bound       stops

An array of `1, 2, 3, 4` that pops twice on the first step, bound 4:

    position 0, 0 < 4, 0 < count 4          visits 1
    advance                                 position 1
    position 1, 1 < 4, 1 < count 2          visits 2
    advance                                 position 2
    2 < 4 holds, 2 < count 2 fails          stops

**Addresses and provenance.** An address is a value. An address into
a dynamic array carries the array value it came from, as provenance.
§68.2 item 9 requires every instruction that can move an array's
storage to name the arrays it invalidates. **An address whose base is
invalidated is not used again.** The lowering re-computes it. An
interpreter poisons it, and a use of a poisoned address is an error
that names the instruction and the invalidation. Neither tier
performs that check, so the interpreter is the only place it exists.

**Suspension and resume.** `Suspend` is a terminator with a successor
block id (§68.2 item 7). **The successor block's parameters are the
live-in set of the suspension.** When the suspension produces a
value, that value is the successor's first parameter. The frame holds
exactly the successor's parameters and nothing else.

**`Suspend` carries its source position.** *(Added 2026-08-26 after
step 2.)* A resume after a hot reload raises `StaleCoroutine`, and
the position it reports is the suspension's. `Trap` carries a
position for the same reason (§68.7.5). The total check of §68.2 item
12 reported 49 sites once it knew to look.

**`Suspend` carries an argument list, as every other edge does.**
*(Added 2026-08-26. Step 1b reported that the section decided what
the frame holds and not how a value reaches it. Reusing a value's id
as a successor parameter breaks §68.1 item 4, and the edge-transfer
paragraph named only the three branching terminators.)* The arguments
bind to the successor's parameters by position, at the moment the
coroutine resumes. A resume value, where the suspension produces one,
is the first parameter and has no argument: the resume supplies it. *(This decides
the case the second step 1 review raised: a resume block had no place
to carry state other than the resume value.)*

**Edge argument transfer.** `Branch`, `ConditionalBranch`, and
`Switch` each carry an argument list per edge. The arguments bind to
the destination block's parameters, by position, at the moment the
edge is taken. The arguments are read before any binding happens, so
a swap across an edge is well defined.

*(2026-08-28, review of §68 form, M6; the bound sentence revised
2026-08-29 when C13 retired.)* The interpreter implemented a different
machine: it read the bound when `IteratorBound` executed, moved the
cursor in `IteratorAdvance` and skipped dead positions inside
`IteratorHasNext`, which wrote to the cursor. **The text above is the
contract.** `IteratorHasNext` is pure: a cursor is an SSA value (§68.1
item 4) and no instruction mutates it. The skip over dead positions is
`IteratorAdvance`'s. The bound is the spelling's, as the `IteratorBound`
row states.

#### 68.7.5 The terminators

| terminator | means |
|---|---|
| `Branch` | the single edge is taken. |
| `ConditionalBranch` | the condition is a `boolean` value. True takes the first edge. |
| `Switch` | the discriminant is compared for equality against each arm's constant, in order. The first equal arm is taken, and the default arm otherwise. |
| `Return` | the function ends. A value is returned when the signature declares one. A coroutine's return completes it. |
| `Trap` | the program ends through the observer of §18, with the named kind and position. |
| `Suspend` | see §68.7.4. |
| `Unreachable` | *(Added 2026-08-28, review of §68 form, M2.)* a successor that checked semantics prove unreachable. It is structural: it carries no trap site and no language meaning. Reaching it is an internal error of the lowering, and the interpreter reports it as invalid LIR, never as a program trap. The text golden prints it as itself. |

#### 68.7.5a What step 1b measured

*(Added 2026-08-26.)* The interpreter ran 98 corpus entries, matched
68 against the golden, and reported 30 disagreements. 51 entries are
declared exclusions, almost all of them interop entries that need the
synthetic native library.

Every disagreement fell into four groups, and each group is one
cause:

1. **23 entries: a field of an aggregate value.** §68.7.2's field
   rows named only the reference-class path. Corrected above.
2. **5 entries: a suspension's successor did not declare the values
   used after it.** §68.7.4 decides that the successor's block
   parameters are the live-in set and that the frame holds those and
   nothing else. The lowering does not build the successor that way,
   so the interpreter, following this section, discarded the values.
   **This blocks step 2.** Both tiers work today because both read
   HIR; a dev tier that reads LIR loses the state. The second step 1
   review predicted this as its M7, from reading. The interpreter
   measured it, on `a110`, `a139`, `a143`, `a145`, and `a149`.
3. **1 entry: the iteration contract contradicted `a80`.** Corrected
   above.
4. **1 entry: the standard runner.** §26.3 requires the runner to
   invoke every exported zero-parameter async function; the
   interpreter invoked `main` only. That is an incompleteness of the
   interpreter, not of LIR.

Groups 1 and 3 are defects in this section. Group 2 is a defect in
the lowering, and it is the one no gate could see, because nothing
consumed LIR.

#### 68.7.6 Language gaps this exercise found

These are gaps in the **language**, not in LIR. Each belongs to the
section that owns the construct, and each needs an owner decision.
LIR carries whatever that section decides.

1. **A container that changes while a `for...of` runs.** *(Corrected
   2026-08-26 after step 1b. This entry said the language had not
   decided it. The language had:
   `corpus/accept/a80-for-of-foreach-mutation` states "appends do not
   extend and removals shorten" and pins both. The corpus is the
   executable definition, so this is a **decided divergence with no
   collision entry**, not an open decision. It belongs to §69, not to
   the owner's queue.)* Measured 2026-08-26 on this host, on an array
   of `1, 2, 3` that pushes `4` on the first step:

       node        1 2 3 4   len=4
       subscript   1 2 3     len=4

   The array grows on both sides. `node` re-reads the length each
   step, and this compiler reads the bound once. **a80 decides this,
   so no decision is open.** `stdlib.md` §14.3 decided the fused
   index loop, and a80 decided the mutation rule that follows from
   it. The work is to give `collisions.md` the entry, under §69. No
   behaviour moves.

   The cost is worth recording, because it is why the two answers are
   not interchangeable: a live bound re-reads the base and the length
   every step, and §68.2 item 9 then re-materializes the base address
   every step, on the loop that `a22-matrix-propagation` measures.
2. **The temporal-dead-zone resolution order** (§66 measurement 6i).
   `node` prints `4` and this compiler prints `3`. No entry of
   `collisions.md` names it. **Owner decision open.**
3. **The order of module initialization.** HIR splits global
   initializers from top-level statements, so source order is not
   recoverable. Neither tier emits top-level statements today.
   **Owner decision open.**

*(§66 measurement 6j, the missing duplicate-declaration diagnostic,
was on this list and is closed. §67 pass A rejects a duplicate
declaration, and `corpus/reject/r149`, `r150`, and `r151` pin it at
S100. Three gaps stay, not four.)*

## 69. The language definition, checked instead of asserted

Origin: the owner asked on 2026-08-26 whether a language rule can be
checked instead of written as prose, and named `node` and `tsc`
output as the oracle to check against. **No language surface moves.**
This section adds checks. It decides no collision.

CLAUDE.md now states the boundary: an external implementation is a
**divergence detector**, never an oracle. A disagreement is a defect
in this compiler, or a divergence that `collisions.md` must name. A
disagreement never corrects a golden.

Measurements at `af5697d`, on this host. `node` is v24.18.0 and
`tsc` is 5.9.2.

1. **`collisions.md` is 1221 lines of prose, and nothing checks that
   its list is complete.** The file states where this language
   differs from JavaScript. A divergence this project did not decide
   reads as a decision.
2. **Two divergences are measured and are in no collision entry.**
   §66 measurement 6i: `node` resolves a name in the temporal dead
   zone as `4` and this compiler as `3`. §66 measurement 6j: the
   duplicate-declaration diagnostic. Both sit in a tracking file.
3. **32 entries assert what `tsc` does, by hand, in three
   spellings.** *(Re-measured 2026-08-27 at `e598994`: 303 entries,
   152 accept and 151 reject.)* `tsc-clean-standalone` appears 25
   times, `tsc-status` 6 times, and `tsc-clean` once. One concept
   with three words breaks this project's own rule that one concept
   takes one word. The other 271 entries assert nothing, so a reader
   cannot tell a measured silence from an unasked question. This
   session got the direction of invariant 5 wrong once, and the
   coding agent found it.
3a. **The hand measurements are already in the headers, unrepeated.**
   One reads "exit 2 (TS2345 at 22:14) verified with
   `node_modules/.bin/tsc --noEmit --strict ...`". A person ran that
   once and wrote the answer down. Nothing re-runs it, so it is a
   claim about another system that the gate does not hold — which
   CLAUDE.md's rule about running another system exists to prevent.
3b. **13 reject entries state no `expected-error` at all**:
   `r52`-`r59`, `r121`-`r123`, `r138`, and `r139`. A reject entry
   that names no diagnostic pins the rejection and not its reason,
   so a later change can reject it for a different reason and the
   gate stays green.
4. **`r153`'s header already records a `node` observation by hand** —
   "node reports a temporal-dead-zone ReferenceError". The work
   below turns that kind of note into a measurement.
5. **78 of 148 accept entries use no `Context`, no `@CStruct`, no
   `FixedArray`, and no foreign call.** That is the rough upper
   bound of the comparable subset. The real number is lower, because
   an entry that depends on integer wrap or on a trap is not
   comparable either.

### 69.1 Three stages

**Stage 1 — every `tsc` claim becomes a measurement.** The gate runs
`tsc` on every corpus entry. An accept entry type-checks. A reject
entry's header states what `tsc` does, and the gate confirms it. A
header that disagrees with `tsc` fails the build.

**Stage 2 — `node` runs the comparable subset.** Each accept entry
carries a `js-comparable` header. The gate runs the comparable
entries under `node` and compares the output against the committed
golden, byte for byte.

**Stage 3 — the collision table becomes an index.** Each collision
carries an id. Each `js-comparable: no` cites one. Each id has at
least one corpus entry. The gate checks all three.

*(Measured 2026-08-27.)* `collisions.md` already numbers C1 to C12,
and §2 carries the Q-register resolutions separately. So stage 3 is
smaller than this section first assumed: the ids exist, and the work
is the two directions of the check, plus ids for whatever §2 decides
that C1 to C12 do not cover.

### 69.2 The headers are the data

1. `js-comparable: yes` — the entry runs under `node` and prints the
   golden.
2. `js-comparable: no <collision-id> …` — the entry diverges by
   decision. It names every collision that applies.
3. **There is no third state.** An accept entry with no
   `js-comparable` header fails the build. 148 entries each get a
   decision, and "not looked at" is not one of them.
4. A reject entry's `tsc` header states `accepts` or `rejects`, and
   the diagnostic code when `tsc` rejects.

### 69.3 The `node` harness

1. **One JS file implements the ambient surface** the comparable
   subset uses. It is small, because the subset avoids `Context`,
   value classes, and foreign calls.
2. **The shim never grows to make an entry comparable.** An entry
   that needs a shim the file does not have is `js-comparable: no`,
   with the reason. A shim that emulates a decided divergence would
   hide the divergence, which is the opposite of the goal.
3. **A total check ties the shim to the prelude.** Every name the
   shim defines exists in `prelude/lang.d.ts`. A shim that drifts
   from the prelude tests nothing.
4. **`tsc` is pinned exactly. `node` is pinned to its major line.**
   *(Corrected 2026-08-28. This read "`node` and `tsc` are pinned",
   and the harness read it as one exact equality for both. The two are
   not symmetric, and one rule for both states a requirement neither
   owns.)*

   `package.json` and its lockfile **install** `tsc`, so the repository
   controls that version, and an exact check compares the record against
   something the repository put there. It fails only for a stale
   `node_modules`, which is a real defect and a cheap one to report.

   The repository does not install `node`. `node` is the host's
   interpreter, and `engines` declares a version rather than supplying
   one. An exact equality therefore fails on every host that has not
   matched a patch release by hand — a failure that reports the host, not
   a divergence, and that stops the whole gate from running.

5. **The golden comparison detects a `node` divergence; the version does
   not.** If `node` changes an observable, §69.5 criterion 4 fails and
   names the entry and the bytes. A version equality adds no detection.
   It moves attribution earlier, and it fires on the runs where nothing
   differs at all. The major line is what the pin must hold, because a
   major release brings a new V8, and that is when a person re-measures
   the record rather than reads past it.

6. **The record states the measured version, not the pinned one**
   (§69.5 criterion 6). The gate prints the `node` and `tsc` versions it
   ran, so the record follows the run. A failure on the major line names
   the version the record was measured on, so a reader can tell a host
   mismatch from a divergence without leaving the message.

### 69.4 What a disagreement means

A disagreement between `node` and the golden is one of two things,
and the round decides which and reports it:

1. **A defect in this compiler.** Fix it. The golden moves only
   because the compiler was wrong.
2. **A divergence this project decided.** Add the collision entry,
   with the measured outputs of both sides, and mark the corpus
   entry `js-comparable: no` citing it.

**A disagreement never corrects a golden on its own.** `node` is not
the oracle. Where this language decides to differ — integer types,
value types, a trap where JavaScript gives `undefined` — `node` is
wrong about this language, and the collision entry says so.

### 69.5 Corpus and gate (pre-registered exit criteria)

1. Every reject entry's `tsc` header is measured, and every accept
   entry type-checks. 151 and 148 at this pin.
2. Every accept entry carries a `js-comparable` header. No entry is
   undecided.
3. Every `js-comparable: no` cites an id that `collisions.md`
   defines. Every id `collisions.md` defines has at least one corpus
   entry. Both directions are checked.
4. Every comparable entry's `node` output equals the golden, byte
   for byte.
5. **The two divergences of measurement 2 gain collision entries**,
   with the measured output of each side. That is the sharpest test
   of this section: it exists because those two were measured and
   never recorded.
6. The record states the count of comparable and non-comparable
   entries, and the `node` and `tsc` versions.
7. Gates: the standing gate is unchanged, and no committed golden or
   `.expected` moves. This section adds checks and moves no output.
8. **Tracking**: `specs/tracking/s69-checked-language-rules.md`.

## 70. A held async handle, by reference count

Origin: the owner asked on 2026-08-27 whether a `@Shared`-style
decorator with a reference-counted handle relaxes the async
restrictions. It does. **No `Promise` object appears, and no
scheduler.** C8's model is unchanged; this section changes who may
hold the frame.

### 70.0 What this relaxes

C8 accepts `async`/`await` as poll-driven sugar over Context-owned
frames, and `r100` and `r105` reject a **floating async call**: the
result must be awaited at the call site. So a program cannot start
work, do something else, and await later.

    const t = doWork();   // r100 today
    stepRenderer();
    const v = await t;

The reason is ownership. The frame is Context-owned and its lifetime
is tied to the await; a held handle has no owner. A reference count
answers that.

**The relaxed surface is already `tsc`-clean.** `Promise<T>` is the
`tsc` view of an async function's value (C8), so holding one and
awaiting it later is valid TypeScript. This section accepts more of
what `tsc` already accepts, which invariant 5 permits without a gate
change.

### 70.1 Owner decisions, 2026-08-27

1. **Scope: the async handle only.** The reference count applies to a
   coroutine frame handle. A user-facing `@Shared` decorator on an
   arbitrary reference class is **not** in this section. It is the
   general form of the same mechanism and it waits for evidence.
2. **At least one `await` is required.** Holding a handle, storing it,
   and passing it are legal. **Dropping it without awaiting is
   rejected.** `r100`'s intent stands: a coroutine that never
   completes runs none of its effects, and a silent no-op is the bug
   that rule exists to prevent. `r100` and `r105` are rewritten, not
   deleted: they reject a *dropped* handle rather than a held one.

### 70.2 Where the count lives

**Measured 2026-08-27.** The allocation header is 16 bytes, fully
packed: an 8-byte state word at `-16`, a class id at `-8`, and a
position id at `-4`. Generated code reads the first two directly.

**The header does not move.** A coroutine frame already begins with

    typedef struct { int32_t state; uint32_t reserved; SubAsyncResume resume; }

and the count goes at byte offset 4. **The frame does not grow, the
header does not change, and no emitted offset moves.**

*(Corrected 2026-08-27. This section first called offset 4 "four bytes
of alignment padding that nothing reads", from reading the struct
declaration in `cemit.rs` and not looking for a writer.* `runtime/src/
context.rs` documented it as the **reload epoch**, and as an "ABI
contract with generated code". One `grep` in `runtime/` would have
found it. The round took the wrong premise and handled it correctly:
an async frame's epoch moved to Context metadata, and a generator
still uses offset 4.)*

*(The owner allowed the allocation header's offsets to move, because
no user depends on binary compatibility yet. That allowance is
recorded and unused here. A user-facing `@Shared` would need it,
because an arbitrary class's allocation has no spare word.)*

### 70.3 The rules

1. **A handle's count starts at one**, held by the value the call
   returns.
2. **A copy increments; a scope exit decrements.** The compiler emits
   both. There is no user-visible operation.

   **2a. Every store of a counted value is one path, and the verifier
   checks it.** *(Added 2026-08-28 after the Fable phase review of
   §69–§70, finding C1.)* Rule 2 says "a copy increments" and the
   lowering acquired at five sites: a local declaration, a local
   assignment, an array-literal element, a call argument, and a
   `return`. A store into a global, a class field, an array element,
   and a spread literal did not acquire. `release_scopes_from` released
   the local regardless, so the count reached zero while the other
   copy still named the frame. Measured at `7bf2559`, each accepted by
   the checker, each followed by four more awaits so the freed frame
   is reused:

       g = t          (module-level `let g: Promise<i32>`)   v=100, expected v=3
       this.h = h     (a class field)                        v=100, expected v=5
       hs[0] = t      (an index store)                       v=100, expected v=7
       [...hs]        (a spread literal)                     o=101 o1=100, expected o=1 o1=2

   Both tiers print the wrong value. Without the reuse step the dev
   tier dies with SIGSEGV and the ship tier runs the frame twice. The
   interpreter reports `unknown packed async handle` for the first
   three and agrees with the wrong output for the fourth, so the
   spread form is a defect all three share (core principle 12).

   This is a class, not four sites, and CLAUDE.md's two-round rule
   applies. The fix is two things:

   - **One path.** Every instruction the lowering emits that stores a
     counted operand into a location that outlives the expression —
     a local, a global, a field, an element, a literal, a spread, an
     argument, a return — is emitted by one function, and that
     function acquires. No store site calls the emitter directly.
   - **A total check.** The LIR verifier walks every function and,
     for every store of an operand whose type is counted, requires
     that the operand is a fresh owner used exactly once, or that its
     retain precedes the store. A violation names the instruction. A
     unit test builds the violating LIR by hand and reads the message
     (core principle 9), and `a161` is Red against the pin.

   **2b. One analysis owns "which expression copies a handle".**
   *(Same review, finding M4.)* The checker's must-await analysis
   (`expr_async_origins`) and the lowering's ownership analysis
   (`acquire_owner`) walk two different site sets. Where they
   disagree the outcome is a false rejection or the use-after-free
   above. Measured false rejections: a `for...of` over a handle array
   (S013 on every element); an arrow lambda that returns a handle;
   `hs[0] = t` awaited only through `hs[0]`. Measured leaks with no
   diagnostic: `flag ? quiet(1) : quiet(2)` retains a count the arm
   already gave (`live_bytes` 24, expected 0); `hs.pop();` as a
   statement never releases the popped element.

   The set of copy sites is one fact. Both analyses read it from one
   place, and a site absent from that place is a build failure, not a
   silent gap. Which form that takes is the round's to choose; the
   property is the contract.

   Corpus: `a161` pins the four stores of 2a with the reuse step, so
   a wrong value is visible; `a162` pins the `for...of` loop, the
   arrow lambda, the index-store-then-await, the conditional
   expression, and `pop()` as a statement, each with `live_bytes`
   read at the end. Both are Red at `7bf2559`.
3. **A count reaching zero frees the frame**, deterministically, at
   the decrement. No traversal runs and no collector is invoked, so
   invariant 2 holds: this is `delete` at a known point, not a
   collector running unbidden.

   **Measured 2026-08-27: today a coroutine frame is never freed.**
   The emitted C for `a93-async-chain` calls `subscript_rt_free` zero
   times, and a frame is allocated with class id `CLASS_GENERATOR`
   and left to the Context's lifetime. A program that awaits a
   million async calls holds a million frames until the host
   collects.

   So this section does not only decide *who* holds a frame; **it is
   the first thing that frees one.** That is a behaviour change and
   it is recorded here rather than discovered in a measurement: peak
   Context memory for an async-heavy program falls, and the fall is
   the point, not a side effect. §70.4 item 6 pins it.
4. **`await` consumes a handle's completion, not its ownership.** A
   second holder still holds it after the first awaits.
5. **A handle is not a `Promise`.** It has no `then`, no combinator,
   and no constructor. C8's rejections stand.
6. **A cycle leaks**, and a program that leaks is correct, merely
   larger — invariant 2's own words. A frame cannot hold a handle to
   itself today; if a shape appears that can, it is recorded, not
   collected.
7. **Workers are unaffected.** Q35 gives per-Context isolation and
   copy-only messaging, so no count crosses a thread and no atomic is
   needed.

### 70.4 Corpus and gate (pre-registered exit criteria)

1. **Red first, at the contract pin.** Each entry below fails at the
   pin, verified against a binary built from it (CLAUDE.md core
   principle 10).
2. `corpus/accept/a154-held-async-handle`: start two async calls,
   do work between them, await both, and print an order that pins
   which ran when.
3. `corpus/accept/a155-async-handle-array`: hold handles in an array
   and await them in a loop.
4. `corpus/reject/r157-dropped-async-handle`: a handle that is never
   awaited. `r100` and `r105` are rewritten to reject the dropped
   form, and their headers record that the held form is now legal.
5. **The count is measured, not asserted.** A unit test reads the
   frame's count through the emitted layout and pins its value across
   a copy, a scope exit, and an await.
6. **Free is deterministic.** A test shows the frame freed at the
   decrement that reaches zero, with no `Context.collect()`.
7. **`a22` stays at or below 1.53×.** The count costs an increment and
   a decrement per handle copy; `a22` holds no async handle, so a
   change there means something else moved.
8. Gates: the standing gate, both profiles, zero warnings, `cargo fmt
   --check`, the `tsc` gate, clippy at the recorded baseline.
9. **Tracking**: `specs/tracking/s70-held-async-handle.md`.

## 71. Static members

Owner decision 2026-08-29 ("1,2,3 やりましょう", item 3), after the
member-namespace measurement under §67.1 rule 3a. Static fields,
methods, and accessors were rejected with S100 as "not decided". `tsc`
accepts them, and a consumer that generates classes meets the
rejection. This section decides them.

### 71.1 The rules

1. **A static member lives in the class's static namespace.** The
   static namespace and the instance namespace are separate, as `tsc`
   has them: `static x` beside `x` is legal. Two static members of one
   name fail with S100 at the second declaration (§67.1 rule 3a, per
   namespace).
2. **A static field is one storage per class, for the module's
   lifetime.** It is a module data binding for §67.1 rule 4c: its
   initializer runs at the class declaration's position among the
   module's statements, in declaration order within the class, and
   reads only bindings declared before it. It survives a hot reload
   as a module global does (§reload). `static readonly` is a `const`
   binding; a write to it fails as a write to a `const` does.
3. **A static method is a function with no receiver.** `this` inside
   a static method or static accessor fails with S100. Stock `tsc`
   binds `this` to the constructor there; this language has no class
   object, and the narrowing is recorded, not measured against `node`.
4. **A static accessor pair follows §65** in the static namespace:
   one name, `get` and `set` of one type, read as `C.x`, written as
   `C.x = v`.
5. **Access is through the class name only.** `C.x`, `C.m()`, `C.x =
   v`. Access through an instance (`c.x` where `x` is static) fails
   with S100; `tsc` reports TS2576 for it.
6. **Where static members are legal.** A reference class and a
   `@CStruct` value class (a static field does not change the
   instance layout). A generic class fails with S100 at the `static`
   keyword: `tsc` gives one storage per class, not per instantiation,
   and this language has no class object to hang it on; recorded as a
   narrowing. A `declare class` (mirror) and a `@Descriptor` class
   keep their existing rejections.
7. **Lowering.** A static field lowers to a module global; a static
   method to a free function; a static accessor to two free
   functions. Both tiers read the same LIR forms they already read
   for globals and free functions; no tier gains a new form.

### 71.2 Corpus

- `a168` (accept): a static field read and written through the class
  name, a static method, a static accessor pair, a static and an
  instance member of one name, a static initializer that reads an
  earlier module binding and an earlier static field, and
  `static readonly`. `js-comparable: yes` if the output matches
  `node`; measure it.
- `r164` (reject): two static members of one name (`tsc` TS2300).
- `r165` (reject): `this` in a static method (`tsc` accepts; recorded
  as a narrowing).
- `r166` (reject): a static member read through an instance (`tsc`
  TS2576).
- `r167` (reject): a static member on a generic class (`tsc` accepts a
  static field that names no type parameter; narrowing).

### 71.3 Exit criteria

1. `a168` byte-identical across dev, ship, interpreter, and golden.
2. Each reject entry's header carries the measured `tsc` code.
3. `tsc` accepts `a168` under the corpus gate's options.
4. No existing corpus entry, golden, or `.expected` moves.
5. Rule 4c's fixpoint treats a static field as a module data binding:
   a static initializer that reads a later module binding through a
   function is rejected (one more shape in `r159`'s family; fold it
   into `r165`'s file only if the diagnostic is the same — else a
   fifth reject entry, `r168`).
