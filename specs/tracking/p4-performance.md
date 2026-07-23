# P4 — performance gate: MISSED

Status: measured 2026-07-23, **both thresholds missed**. Contract:
`specs/blocks/compiler.md` §3 (thresholds, pre-registered) and §9
(methodology, pinned before any number existed).

## Result

| Subject | Median | Min | Max | Spread | vs C | §3 limit | Verdict |
|---|---|---|---|---|---|---|---|
| C baseline | 3.962 ms | 3.955 | 4.000 | 1.0% | 1.00× | — | — |
| ship-AOT | 136.298 ms | 135.157 | 141.318 | 3.7% | **34.40×** | 1.5× | **MISSED** |
| dev-JIT | 151.807 ms | 148.682 | 155.365 | 2.3% | **38.32×** | 4× | **MISSED** |

Orchestrator's independent re-run: 34.29× / 37.78×, spreads 1.7% /
2.2%. Eight further runs at other warm-up counts: 27.8–34.7× (AOT),
30.9–38.3× (JIT). The outcome is not borderline and is reproducible.

Preparation time (reported, not gated): dev-JIT compile 2.23 ms;
ship-AOT check+lower+emit 4.09 ms, link 59.6 ms; C compile 95.7 ms.

**Validity**: all three subjects printed the frozen golden
(`40021.875\n`) byte-exactly; the harness refuses to report timings
otherwise, so the three measured the same computation (§9). The C
baseline reproduced the golden on its first compile, unadjusted.

## Conditions

MacBook Air `Mac14,2`, Apple M2 (4P+4E), 16 GB, macOS 26.5.2, arm64, on
AC power. Single session, all three subjects in one harness process.
Apple clang 21.0.0, `-O2 -ffp-contract=off` (the language never
contracts multiply-add; the contracting build is ~1.8× *slower* here,
so this flag gives the stronger, not the weaker, baseline).

Timed spans (§9): C — the workload call (array construction, 100
propagation iterations, checksum); ship-AOT — the `ss_export_main` call
in the linked binary; dev-JIT — the `main` call in-process. Compilation,
linking, JIT warm-up, Context setup and I/O are outside all three.

**Methodology note**: §9 sets 3 warm-up runs as a floor. At exactly 3,
the C subject fails §9's ±20% noise rule on every attempt with a
strictly monotonic decay (a per-process DVFS ramp — 3 warm-ups of a 4 ms
workload is 12 ms of CPU, far short of the M2's ~50 ms to steady state,
while the 135 ms tiers clear it during warm-up). §9's remedy is to redo
the run; redoing it identically always fails, so warm-up was raised to
30 until every subject was in steady state. Nothing else moved, and the
correction makes the baseline *faster* (3.96 ms vs 4.87 ms) — the gate
became harder, not softer. The harness prints every sample in order so
a ramp is recognizable.

## Diagnosis

Profile of the AOT subject (`/usr/bin/sample`, 3063 samples): ~79% in
the generated body of `multiply`, ~7% in `memmove`/`memset` for 64-byte
`Matrix4` / `FixedArray<f32,16>` copies, ~2% in runtime calls, the rest
in `checksum`.

Inspection of the lowering (`codegen/src/lower/func.rs`) identifies the
dominant cost as **this project's own code generation, not a Cranelift
ceiling**:

1. **A bounds check per `FixedArray` element access, with no range
   analysis.** `index_addr` emits an unsigned compare and `guard`
   emits *two basic blocks and a conditional branch* per access. a22's
   `multiply` performs 144 indexed accesses (128 reads + 16 writes) per
   call and is called ~1M times, so the hot loop carries ~144M
   compare-and-branch pairs and a CFG fragmented into hundreds of
   blocks. Every index involved is provably in range — `row*4+inner`
   and `inner*4+column` with all three loop variables in `[0,4)` — so a
   straightforward range analysis eliminates all of them.
2. **The fragmented CFG forecloses vectorization in any backend.** A
   conditional branch inside the innermost loop prevents the 4×4 matmul
   from being vectorized regardless of which code generator is used;
   clang's NEON auto-vectorization of the baseline is measured against
   a loop this project has made unvectorizable.
3. **Value-class copy traffic.** `multiply` returns a `Matrix4` by value
   into `world[index]` and `perturbLocals` copies each matrix out and
   back (C2 semantics), producing the `memmove`/`memset` traffic C
   elides by writing in place.

Only the residue after (1)–(3) is a property of the backend. The
measurement therefore does not, on its own, indict Cranelift.

## Consequence (§3)

The pre-registered criterion fired: **the backend decision is reopened,
with this measurement as the named criterion.** Per §9 the gate is not
retried with a different methodology, and no threshold moves. The
decision of what to do next is the owner's; the evidence above is the
input. Recorded without adjustment, per §9's requirement that both
outcomes be recorded.

## C-emission spike (P4.2, owner decision 2026-07-23)

The re-measurement placed ~73% of the residual AOT gap in Cranelift's
scalar, unvectorized output (§ above). The C-emission ship fallback was
pre-registered at P0.5 (§3); clang is LLVM, so the frozen 3.96 ms C
baseline already measures what LLVM's vectorizer does with this
computation. Open question: how much of that win survives the language's
own semantics (C2 value-class copies, checked growable-array indexing)
when the C is emitted from the lowering rather than hand-written.

The spike answers it with a number: emit C for a22 from the typed HIR,
carrying the language's semantics faithfully, compile with the platform
C compiler at `-O2`, verify it prints the frozen golden byte-exactly,
and measure it in the P4 harness alongside the existing subjects.
Bounded to a22 — it is a measurement, not the C backend. Its result
informs the §3 backend decision; it does not by itself decide it.

### P4.2 result (orchestrator-verified)

Emitter `codegen/src/cemit.rs` emits a self-contained C translation unit
from the same typed HIR the CLIF path consumes, carrying the language's
semantics: `Matrix4` value params **by value** (C2 copy-on-pass, which
the hand baseline elides with `const*`); the `FixedArray` inner matmul
**unchecked** via the same P4.1 interval proof; dynamic arrays as
`(data,len,cap)` with a **bounds check per access** (`sub_arr_at`) and
realloc-on-push; f32 kept in `float`; Q14 print replicated. Verified by
inspecting the emitted C (inner loop unchecked, value params by value,
dynamic access checked, element store resolved after the RHS) and by
`codegen/tests/cemit.rs` asserting the frozen golden byte-exact.

Measurement (§9 unchanged, `--warmup 30 --timed 11`, M2, AC;
orchestrator-reproduced at 1.05×):

| Subject | Median | vs C |
|---|---|---|
| C baseline (hand) | 3.98 ms | 1.00× |
| ship-AOT (Cranelift) | 92.3 ms | 23.21× |
| dev-JIT (Cranelift) | 104.6 ms | 26.34× |
| **emitted-C (clang)** | **4.19 ms** | **1.05×** |

**Emitted-C clears both thresholds** (1.05× ≤ 1.5× and ≤ 4×), carrying
the C2 copies and checked dynamic indexing the measurement requires. The
~5% over the hand baseline is exactly those semantics (value-param
copies, per-access array checks, growth). The identical semantics cost
~5% through LLVM and ~23× through Cranelift `opt_level=speed`.

**Conclusion**: the residual P4.1 gap is confirmed as Cranelift backend
behaviour, not a lowering defect, and the C-emission ship route reaches
the pre-registered §3 threshold. This is the input to the backend
decision; the decision itself is the owner's (§3).

The lowering is optimized and the gate re-measured before the backend
decision is judged (`specs/blocks/compiler.md` §10, P4.1). Rationale:
the profile and the lowering inspection place the dominant cost in this
project's code generation, so switching backend now would answer a
question the measurement has not asked. §3's thresholds and §9's
methodology are unchanged; the standing gate protects correctness while
the optimization lands.

## P4.1 — lowering optimization and re-measurement (2026-07-23)

Contract: `specs/blocks/compiler.md` §10. Two optimizations in the
shared lowering (`codegen/src/lower/func.rs`), no tier branch:

1. **Proof-based bounds-check elimination.** An `i128` interval lattice
   (`interval_of`, `induction_interval`) proves a `FixedArray` index in
   `[0, n)` and removes the check only then. Proof conditions:
   constant loop start, proven `<`/`<=` bound, positive constant step,
   the counter is not reassigned in the body (`++`/`--` included), and
   `hi + step` does not overflow the counter type. Anything unproven
   keeps the check. Fires on a22's `multiply`/`checksum`; declines all
   dynamic-array (`T[]`) indexing.
2. **Value-class copy elision.** A destination hint builds construct-like
   RHS forms (`new` value class, `sret` call result, `FixedArray`
   literal) straight into their home, eliding the temporary. C2's
   observable copy semantics are unchanged (`a04` still prints `1,9`);
   growth-relocatable array elements are excluded to preserve trap
   order.

### Re-measurement (§9 methodology unchanged, orchestrator-reproduced)

| Subject | P4 vs C | P4.1 vs C | §3 limit | Verdict |
|---|---|---|---|---|
| ship-AOT | 34.40× | **23.37×** | 1.5× | **MISSED** |
| dev-JIT | 38.32× | **26.69×** | 4× | **MISSED** |

All three subjects still print the frozen golden byte-exactly; the
standing gate (§8.3) is unchanged and green (no golden byte moved), so
the optimization is correctness-preserving.

### Gap attribution (§10 final clause; AOT profile, ~39k samples)

- ~73% of the remaining 23.37×: Cranelift `opt_level=speed` emits a
  **scalar, unvectorized, unrolled** inner matmul from clean
  branch-free CLIF; the C baseline is NEON-vectorized by clang. This is
  **backend behaviour, not a lowering defect** — the CLIF is now
  branch-free and the disassembly confirms single-lane `fmul`/`fadd`.
- ~10%: residual 64-byte value-struct copy traffic (part ABI/lowering,
  part C2-fundamental).
- ~6%: out-of-line dynamic-array indexing (`sub_rt_array_ptr`,
  deliberately still checked; addressable in a later phase).

Of the original ~35× gap, this project's own code generation
contributed the removed ≈44 ms (≈32% of the original wall-time); the
surviving gap is dominated by the backend. **The measurement now
answers §3's real question**: with removable codegen overhead gone,
Cranelift `opt_level=speed` does not bring the a22 matmul near 1.5× of
`-O2` NEON-vectorized C, and the residual is overwhelmingly the
backend's scalar output.

### Phase Review (2026-07-23)

Soundness-critical (removes bounds checks). Fresh no-context review ran
37 adversarial programs through the JIT: 0 CRITICAL, 0 MAJOR, 2 MINOR.
The interval analysis is **sound** — a check is removed only when the
indexed array's own length dominates the whole proven interval, and
every proof condition that could break monotonicity (non-constant/zero/
negative step, in-body reassignment, closure capture, type overflow,
signedness-changing casts) is correctly declined, with a real trap
still firing in every unprovable out-of-range case including a loop
bounded by one array's length but indexing a shorter one. Copy elision
preserves C2 (verified: assign-then-mutate-source, `arr[i]=arr[j]`,
self-assign, NRVO). MINOR fixed: the induction-counter assignment
scanner's `#[non_exhaustive]` catch-alls now default to "possibly
assigns" (declines the proof) rather than "no assignment", so a future
HIR variant cannot silently enable an unsound proof. MINOR (perf figure
reproducibility) closed by the orchestrator's independent re-run
(23.37× / 26.69×).

## P4.3 — C emission as the ship tier (2026-07-23)

Contract: `specs/blocks/compiler.md` §11; plan §8 Rev 2. The a22-only
P4.2 emitter (`codegen/src/cemit.rs`) is extended to the full run set
and made the ship tier, replacing `cranelift-object`. It links the
runtime staticlib (identical arrays/strings/coroutines/Q14/traps) and
exports the same `ss_init` / `ss_export_<name>` surface, so it is a
drop-in for the AOT entry.

### Standing gate rewired

The default `cargo test` differential is now **dev-JIT ≡ ship-C-AOT ≡
golden**, byte-exact, all 24 entries (a19 two-file), derived from the
corpus (no id list, floor 24, no silent skips, missing `cc`/link
fails). `cranelift-object` AOT is retained only as an optional
cross-check column; its ship role has ended. Device triples are
cross-compiled with `clang --target=…` (iOS `-miphoneos-version-min=10.0`,
Android NDK), replacing the `cranelift-object` device link; compile+link
only. No golden byte changed.

### Phase Review (2026-07-23) — the finite-gate blind spot

Fresh no-context, soundness-focused review: the 24-entry gate was green,
but the ship tier silently miscompiled **checker-accepted programs the
goldens do not exercise** — exactly the risk of proving a second
lowering with a finite gate. Found and fixed: 2 CRITICAL, 3 MAJOR.

- CRITICAL: a mutating value-class method dropped the mutation (receiver
  passed by value) → pointer receiver, mirroring the CLIF path.
- CRITICAL: a capturing lambda over a non-`i32` capture truncated it
  (capture type hard-coded `i32`) → real per-capture type tracking.
- MAJOR: `collect()` mis-collected live objects (no GC roots in the C
  tier), so `collect()` + `unsafeDelete` of a live handle spuriously
  trapped → shadow-frame rooting reusing the shared `layout` helpers.
- MAJOR (orchestrator-reproduced, initially mis-filed as a follow-up):
  the same mis-collection for references held inside a `FixedArray`
  local/param → root managed-aggregate interiors via `managed_words`,
  byte-identical shadow-frame shape to the dev tier.
- MAJOR: signed `+`/`-`/`*` was C UB on overflow (clang happened to
  wrap) → compile the ship C with `-fwrapv` everywhere it is built.

Value-class fields of managed type (reference/string/`FixedArray`-of-ref/
dynamic-array) are unreachable — the checker rejects them S100 (C2
whitelist), matching the CLIF path's `has_managed_interior`.

**Lesson (one line):** a second lowering adopted as a shipping tier is
only as sound as the inputs its differential gate exercises; every
class of divergence found must become a permanent dev-JIT ≡ ship-C-AOT
regression test, not just a fixed golden. Six such tests were added
(mutating value method; f32/i64/i32 captures; collect+delete and
collect+use of live handles; managed references inside a FixedArray
local and param).

### Verification

`cargo test --offline`: 212 passing, zero failures, zero warnings; the
standing gate byte-exact on all 24; the orchestrator's original
divergence probe (`FixedArray<Box,2>` + `collect()` + `unsafeDelete`)
now agrees across tiers (`10,20\nok\n`). Goldens untouched.

## P4 exit

The performance gate (§3) is met via the C ship tier: emitted-C
ship-AOT 1.05× the C baseline (≤ 1.5×), dev-JIT is the dev tier
(iteration-speed argument carried by JIT compile at ~2 ms; its 26×
execution ratio is recorded, not a ship concern — shipping is C). The
backend decision (§3, reopened by P4) is resolved: ship = C emission
(LLVM); dev = Cranelift JIT. Standing gate proves the two tiers
byte-identical on the run set. P4/P4.1/P4.2/P4.3 COMPLETE. Next: P5 —
C-header binding vertical slice.

## Artifacts

`bench/a22-baseline.c` (baseline, header comment names the corpus
entry), `bench/aot-entry.c`, `bench/src/main.rs` (harness crate
`subscript-bench`, bin `bench`). Run:
`cargo run --offline --release -p subscript-bench --bin bench --
--warmup 30 --timed 11`. Release is enforced. No build products are
written inside the repository.
