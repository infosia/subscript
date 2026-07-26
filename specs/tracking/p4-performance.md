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

`benchmarks/a22-baseline.c` (baseline, header comment names the corpus
entry), `benchmarks/aot-entry.c`, `benchmarks/src/bin/perf-gate.rs` (harness crate
`subscript-benchmarks`, bin `perf-gate`). Run:
`cargo run --offline --release -p subscript-benchmarks --bin perf-gate --
--warmup 30 --timed 11`. Release is enforced. No build products are
written inside the repository.

## Follow-up — `tree` allocation cost: ship-tier release (§8.1a)

The cross-language `tree` workload (30 full binary trees, depth 16 —
131,071 reference-class nodes each, built then `unsafeDelete`d in
sequence) was the suite's worst subject: ship 6.72×, dev-JIT 7.98× the C
baseline on x86_64 windows-msvc (clang 22.1.6). Root cause, isolated by a
controlled experiment (DEPTH=16 fixed, COUNT swept 5→30 so the live set is
constant and only the retained-dead count grows):

| COUNT | nodes | C ns/node | ship ns/node (retain) | ship ns/node (release) |
|---|---|---|---|---|
| 5 | 655,355 | 49.4 | 233 | 175 |
| 10 | 1,310,710 | 47.2 | 254 | 173 |
| 20 | 2,621,420 | 47.4 | 335 | 190 |
| 30 | 3,932,130 | 46.5 | 365 | 210 |

C is flat; the retain policy rose +57% across the range. Cause:
`Context::delete` marked the allocation dead and kept it in the
`allocations` table for the whole run, so the table grew monotonically to
3.9M entries (cache-hostile inserts). The fix is §8.1a: the ship tier
frees and removes on `unsafeDelete`/`collect`, bounding the table at the
live set. Ship-tier `tree` 6.72×→4.46× at COUNT=30; per-node rise cut from
+57% to +20% (the superlinear term removed). The dev tier is unchanged by
design (still retains-and-poisons to trap use-after-delete), so dev-JIT
`tree` stays ~8× — the dev tier's trap guarantee, not a ship concern.
`particles` (value-struct AoS, no `unsafeDelete`) unchanged, confirming
scope. The residual ship 4.46× is the per-allocation `HashMap`-op cost
versus C `malloc`; closing it needs a slab/free-list allocator (§8.1a
"why it matters"; a larger change, not scheduled).

Release policy covers **all three** allocation-retire sites, not just
`delete`: `unsafeDelete`, the `collect` sweep, and the `array_push`
capacity-doubling retire of the old data block (a review found the last
site initially still poison-and-retained, which would have re-grown the
table for array-push-heavy ship programs; now fixed — ship frees the
retired block, dev still poisons it). So "bounded at the live set" holds
for arrays too. Verification: full workspace `cargo test` green incl.
`golden.rs` (JIT≡AOT≡golden byte-exact) and the ship-mode runtime unit
tests (`ship_mode_delete_frees_and_removes_the_entry`,
`ship_mode_collect_frees_and_removes_unreachable`,
`ship_mode_array_growth_frees_retired_blocks`).

Recorded design consequence of §8.1a (review MINOR 2, not a defect): the
ship tier's free-on-delete removes the dev tier's property of turning a
codegen shadow-rooting gap or an unguarded use-after-delete into
deterministic, gate-catchable behavior. The string read path
(`print`/`str_bytes`) emits no `live_check`, so a rooting gap would print
the correct golden in dev (retained bytes, no trap) yet corrupt in ship —
a class of latent bug the corpus differential gate cannot see for
ship-only. This is the intended two-tier split (AOT use-after-delete is
undefined, Q6), stated once here so the testability cost is on record.

## P8 — ship-tier arena allocator (§8.1b): COMPLETE (2026-07-24)

The slab/free-list allocator the §8.1a record named as the unscheduled
next step. Measured motivation (scratch attribution bench, 30×131071
alloc/delete pairs of 16-byte payloads): the per-allocation `HashMap`
plus its bookkeeping was ~75% of the ship tier's allocation overhead;
the 32-byte-zeroed-with-header allocation shape itself was ~+17% over
bare `malloc`/`free`.

Mechanism (runtime/src/context.rs, ship Context only; dev tier's map +
retain-and-poison + traps unchanged; no `sub_rt_*` ABI change): 8
power-of-two size classes (32..4096 B total block) carved from 64 KiB
per-class chunks by bump pointer; `delete` pushes the block onto its
class's LIFO free list (link threaded through the freed payload's first
word); above 4096 B an individual `LargeAlloc` record. Membership is
exact per §8.1b — chunk binary search + block grid + bump watermark +
live header, all four. `collect()` marks via a header `MARK_STATE`
magic (restored on sweep) and sweeps by chunk-grid walk plus the large
records. `Drop` frees chunks and records wholesale. Payload zeroed on
every alloc path including free-list reuse (full class capacity).

Exit criteria (§8.1b, pre-registered) — all met:

1. Ship `tree` **1.37× C** (target ≤2.0×, from 5.11×), arm64 reference
   machine, standard runner; no other ship row regressed (sort
   1.79→1.77, particles 3.06→3.07, compute-bound rows within noise).
   Ship now leads LuaJIT on every workload including `tree` (2.20×).
2. Standing gate byte-exact on every corpus entry, both tiers, incl.
   a16 collect (the gate's AOT binaries run the arena end to end).
3. Runtime unit tests 274→281: free-list reuse without chunk growth
   (same-address over 10k cycles), zeroed reuse, Drop releases every
   chunk/large record (test-only ArenaStats balance), arena collect
   rooted/unreachable/transitive, large-path membership/trace/delete,
   exact-membership negatives; dev-tier trap tests unchanged.
4. `is_live`/`live_count` functional on both tiers.

Phase Review (2026-07-24, fresh no-context): 0 CRITICAL, 0 MAJOR, 3
MINOR. Executed probes all passed: array-grow+collect (data block
reached through the ArrayHeader payload word, both tiers); 100k-op
seeded torture across all classes + large threshold with periodic
collect vs a shadow model (zero divergence); interning under
collect/delete churn; the tree pattern (iterations 2–30 reuse only
iteration-1 addresses — no chunk growth); zeroing incl. partial-capacity
reuse; membership negatives (chunk base, header address, payload
interior, above-watermark, chunk end, cross-class) all rejected;
retired-block + stale-root + double collect leaves the free list
duplicate-free. MINOR 1 (commit the benchmark evidence + this entry;
regenerated README cited the spec commit's hash — re-run at the
implementation commit, now cites 821170e) resolved with this commit.
MINOR 2 (clippy doc_lazy_continuation in a test doc comment) and
MINOR 3 (pre-existing P5.2b `vec_box` on `callbacks` — the Box is
load-bearing: `bind_callback` returns a stable interior pointer; wants
a justifying `#[allow]` comment) recorded as cosmetic follow-ups.

Residual ship `tree` gap (1.37×): the `sub_rt` call boundary, header
writes, and full-capacity re-zeroing on reuse — not the map (gone).
Not scheduled; §8.1b's target is met.

Second-platform confirmation (x86_64 / Windows, clang 22.1.6, 20 logical
cores, same 20 warm-up / 21 timed schedule, snapshot @3dc3695 in
`benchmarks/`): ship `tree` 5.33× → **0.81×** C, i.e. below the C
baseline, reproduced at 0.87× in a second `--only tree` run. The
baseline there is the MSVC UCRT `malloc`/`free`, which is slower than
the arm64 platform allocator, so the ratio is not comparable to arm64's
1.37× — what carries across is that the size-class arena removed the
allocation-count-dependent growth on both platforms. Other ship rows
unchanged within this machine's spread (sort 1.91→1.84, particles
2.24→2.36, compute-bound rows 0.99–1.03×). The dev-JIT rows are
unaffected by §8.1b by construction (one `release_on_delete` branch;
the dev path is the unchanged map + retain-and-poison) and their
movement is run-to-run noise: the same binary produced 1470 ms and
1267 ms for `tree`/subscript-jit on two consecutive runs.

## Cross-language re-measurement, 2026-07-27 (post P19–P21)

Re-ran the whole suite after P19, P20 and P21, and added the `collect`
row. `benchmarks/README.md` is **generated by the runner**, so this
entry carries what the generated file cannot: what three consecutive
runs showed.

### The phases moved two rows, in the direction their mechanism predicts

| workload | 2026-07-25 ship | 2026-07-27 ship |
|---|---:|---:|
| `particles` | 3.06× | **1.93×** |
| `sort` | 1.80× | **1.24×** |
| `tree` | 1.42× | 1.69× |
| `callbacks` | 20.84× | 22.86× |

`particles` and `sort` both have array indexing in their inner loop,
which is what P19 changed: `ss_arr_at` was an inlined helper whose
fallback pointer cost a null test, a `csel`, a live global address and
a reachable cold call, and removing it took the loop body from 82
instructions to 39 (`p19-trap-parity.md`). `tree` is `unsafeDelete`
-dominated with little indexing and moved the other way.

**These are not controlled measurements**, unlike P19's own
before/after pair — the trees differ by three phases. The direction
matches the mechanism; the magnitude is not attributed.

### The noise is specific, not ambient

Three runs at `--warmup` 3, 25 and 60. Invalid cells: **6, 5, 4** —
better with more warm-up, which is what `compiler.md` §9 prescribes
(3 is a floor, not a target). The committed snapshot is the third.

Which cells failed is the useful part:

| cell | run 1 | run 2 | run 3 |
|---|---|---|---|
| `fib-loop`/C | 64% | 49% | 86% |
| `primes`/C | 98% | 52% | 52% |
| `mandelbrot`/C | ok | 25% | 27% |
| everything else | rotates | rotates | ok |

**The C subject on `fib-loop`, `mandelbrot` and `primes` is
persistently unstable; nothing else is.** The C subject is otherwise
the most repeatable in the suite — across the same three runs its valid
medians vary by well under 1%:

```
sort/C      15.630  15.320  15.342
tree/C      65.641  65.557  65.559
queen/C     23.838  23.729  23.760
particles/C 38.755  38.781  38.784
callbacks/C 13.120  13.077  13.119
collect/C   32.605  32.565  32.463
```

So this is not "the machine is noisy". Duration does not explain it
either: `callbacks`/C at 13 ms is stable and `mandelbrot`/C at 125 ms
is not, and `mandelbrot` is stable in every *other* subject at the same
125 ms.

**Open, and it matters more than a normal noisy cell: C is the 1.00×
reference.** A subject that cannot be measured on three workloads
cannot anchor their ratios, so those three rows publish absolute
medians with no ratio at all. Worth diagnosing — the C harness's own
timing loop for those three workloads is the place to look, not the
machine.

### `collect`, first capture

`1.00× C (32.5 ms) | ship 6.44× | jit 6.88× | LuaJIT 3.70× | JSC
invalid | V8 2.61×`, checksum `1332546592` reproduced by every subject
that ran. All six can force a collection here — node has
`--expose-gc`, this JSC shell exposes `gc()`, LuaJIT has
`collectgarbage`, C frees explicitly.

## Warm-up was silently zero for three C workloads (2026-07-27)

`clang -O2` **deleted the warm-up loop outright** in `fib-loop`,
`mandelbrot` and `primes`. Their `workload()` takes no argument,
touches no memory and has no side effect, so LLVM proves it pure and
terminating, and the loop's only result is overwritten by the timed
loop. Verified by wall time, `--warmup 0` against `--warmup 200`:

```
before   fib-loop 0.63s -> 0.09s     primes 0.44s -> 0.06s   (no work added)
         sort     0.40s -> 3.10s                             (200 iterations ran)
after    fib-loop 0.63s -> 5.95s                             (200 x 29ms)
```

**`--warmup` was a complete no-op for exactly those three**, which is
why raising it 3 → 25 → 60 across three full-suite runs never fixed
their spread failures. The contract required "≥3 warm-up runs
discarded" and the real figure was zero for three of ten.

The seven that survived did so **by accident** — heap access, `queen`'s
`volatile`, or recursion defeating the termination proof. Nothing in
the harness kept warm-up alive, so all ten now write the warm-up result
to a `volatile` sink.

Two further corrections came out of it. Warm-up is now a **measured
time floor** (200 ms, ~3× this machine's measured 70 ms DVFS ramp)
because a count cannot express "reach steady state" across 3.7 ms to
125 ms per iteration; and **a subject reports its warm-up time and is
rejected below the floor**, because three full-suite runs could not
diagnose a silently-zero warm-up. `compiler.md` §9 was saying the old
thing while `benchmarks.md` Rev 3 claimed to mirror it.

**Result: the first fully clean capture — all 60 cells valid**, minimum
measured warm-up 0.2009 s.

The instability was never "the machine is noisy". It was a harness bug
plus a 1.63× DVFS ramp, and `cpu/wall ≥ 0.9996` on every sample ruled
out descheduling from the start.

## Open — `collect` on the dev tier is bimodal, 221 ms against 459 ms

| capture | ship | dev-JIT |
|---|---:|---:|
| three pre-fix runs | ~209 ms | **~221 ms** |
| post-fix (coding agent) | 209.1 ms | **221.7 ms** |
| clean capture 1 (published) | 209.4 ms | **462.7 ms** |
| clean capture 2 | 208.9 ms | **459.0 ms** |

**Not noise**: each capture's own 11 samples sit within 1–3% of its
median. The ship tier is stable at ~209 ms across all six. Only the dev
tier moves, and it moves by a factor of two between processes while
being tight inside one.

No code changed between the 221 ms and 459 ms captures — the warm-up
fix touched the C harnesses and the runner only, and warm-up iteration
count for this cell is 3 either way.

**Hypothesis, not yet tested:** collection is **conservative**, so the
mark phase traces every payload word that looks like a live block
address. The dev tier makes an individual system allocation per object
tracked in a map, so its addresses are far more scattered than the ship
tier's arena — which would make spurious traces both more likely and
more sensitive to where the heap lands, i.e. to ASLR. That would
produce exactly this signature: stable within a process, bimodal
across processes, dev tier only.

The published figure is capture 1's 14.21×. **It is a real measurement
of one process and not a stable property of the tier**, which is why
this entry exists.

This is the `collect` workload doing its job on its first outing: the
suite had no way to see any of this before P21's review asked for it.
