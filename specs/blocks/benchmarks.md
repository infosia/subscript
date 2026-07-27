# Cross-language benchmarks — contract

Status: Rev 4, 2026-07-27 (a fresh process per (workload, subject) becomes normative — `subscript-jit` had been the one in-process subject and its whole column was order-dependent); Rev 3, 2026-07-27 (warm-up becomes a measured time floor after clang was found deleting the warm-up loop outright in three C workloads); Rev 2, 2026-07-26 (Rev 0: 2026-07-23; Rev 1 adds the `callbacks` workload, which the P18 Phase Review found the suite had no coverage for; Rev 2 adds `collect`, which the P21 Phase Review found the same way). A cross-language performance comparison of the
subscript ship and dev tiers against a C baseline and JIT-enabled
scripting runtimes. Not a gate (the P4 gate in `compiler.md` §3/§9 is the
gate); this is a published comparison. Lives in `benchmarks/`.

## Purpose and reporting rules

- Report measured numbers, whatever they are. No subject is tuned to
  flatter subscript; no baseline is weakened.
- **The C baseline is 1.00×.** Every other subject is reported as a ratio
  of its median to the C median, plus its absolute median seconds.
- **Same computation, verified.** Every subject computes the same
  workload and prints the same **integer checksum**; the runner refuses
  to report a workload's timings unless every subject's checksum is
  identical. Integer checksums avoid cross-language float-formatting
  mismatch.
- Repo hygiene (CLAUDE.md): the benchmarks name and cite only the
  runtimes actually compared (below), by upstream URL. No predecessor,
  sibling, or external-oracle project is named or referenced. The
  workloads are classic public algorithms implemented fresh here.

## Subjects

| Subject | What it is | How timed |
|---|---|---|
| C | hand-written C, `clang -O2 -fwrapv -ffp-contract=off` | self-timed (monotonic clock), prints median |
| subscript-ship | HIR → C → `clang -std=c11 -O2 -fwrapv -ffp-contract=off` (the ship tier, `compiler.md` §11) | externally timed by the runner (AOT entry loops the exported workload fn) |
| subscript-jit | HIR → Cranelift JIT (the dev tier) | externally timed, in a **fresh runner re-exec child per workload** (`jit_bench`) |
| LuaJIT | [luajit.org](https://luajit.org) 2.1 | self-timed (`os.clock`), prints median |
| JSC | JavaScriptCore, JIT-enabled ([webkit.org](https://webkit.org)) | self-timed (`Date.now`/`performance.now`), prints median |
| V8 | Node.js ([nodejs.org](https://nodejs.org)) | self-timed, prints median |

subscript has no clock primitive, so its two tiers are timed by the
runner, not from inside the script — the C/JS/Lua subjects time
themselves and print `<checksum> <median_seconds>`.

## Timing methodology (mirrors `compiler.md` §9)

- Each subject: **≥11 timed runs, report the median**; also record
  min/max and **every sample**. A spread wider than ±20% of the median
  invalidates that subject's timing, which is withheld.

- **Warm-up is a time floor, not a count: ≥200 ms of measured warm-up
  execution**, and ≥3 iterations. *(Revised 2026-07-27; it said "≥3
  warm-up runs discarded" and was wrong twice over.)*

  A count cannot express "reach steady state" across a suite whose
  per-iteration cost spans 3.7 ms to 125 ms. Measured on the arm64 dev
  machine: the CPU ramps **2.14 → 3.50 GHz, a factor of 1.63, over
  ~70 ms** of continuous load, and `cpu/wall ≥ 0.9996` on every sample
  says the thread is never descheduled — the clock is simply low, so
  this is DVFS and not contention. 200 ms is about three times the
  measured ramp.

  This is not only about the ±20% gate. Under-warmed, `fib-recursive`'s
  C median measured **4.45 ms against 3.65 ms** warm — the *published
  number* was ~20% pessimistic, not merely noisy.

- **A subject must report its measured warm-up time, and the runner
  rejects the subject if it is under the floor.** This exists because
  the floor was silently zero for three of ten C workloads and three
  full-suite runs could not diagnose it.

  `clang -O2` **deleted the warm-up loop entirely** in `fib-loop`,
  `mandelbrot` and `primes`: their `workload()` takes no argument,
  touches no memory and has no side effect, so it is provably pure and
  terminating, and the loop's only result is overwritten by the timed
  loop. Verified by wall time at `--warmup 0` against `--warmup 200`:
  `fib-loop` 0.63 s → 0.09 s and `primes` 0.44 s → 0.06 s — no work
  added — while `sort` went 0.40 s → 3.10 s, which is the 200
  iterations actually running.

  The seven that survived did so **by accident**: `sort`, `tree`,
  `particles`, `callbacks` and `collect` touch the heap, `queen` has a
  `volatile` guarding against constant-folding, and `fib-recursive` is
  recursive so termination cannot be proven. Nothing in the harness
  was keeping warm-up alive.

  Each C harness therefore writes its warm-up result to a `volatile`
  sink. **This is not a workaround for the optimizer — it is what makes
  the contract's own requirement true**, and the reported warm-up time
  is what proves it stayed true.

- **Every `(workload, subject)` measurement runs in a fresh process.**
  *(Normative since 2026-07-27.)* Until then `subscript-jit` was the
  one exception — it ran **in-process** inside the runner, so by
  workload 10 of 10 it inherited a system heap nine workloads had
  churned, while every other subject got a cold process.

  It was not a small effect. `collect` measured **463 ms** in the full
  suite against **226 ms** standalone, the same binary minutes apart,
  with `subscript-ship` unmoved at 213 against 215. After the fix the
  two agree to 0.25% (221.5 against 222.1).

  The dev tier is the sensitive one because it makes **one system
  allocation per object**, so its object layout and locality are
  inherited from whatever the process did before; `collect`'s mark
  phase then pointer-chases across ~100 000 of them. The ship tier was
  immune twice over — a spawned binary, and an arena that places
  objects itself.

  **Three other cells moved when this was fixed** — `particles` +10.7%,
  `sort` +7.1%, `callbacks` +5.3% — and the direction is the tell.
  Running eighth and ninth, they had been *flattered* by a warm
  process; `collect`, running tenth, had been *penalized* by a
  fragmented one. Both are now measured cold, like every other subject.
  The whole `subscript-jit` column had been order-dependent, not just
  the one cell that made it visible.

  The re-exec child runs exactly one workload and reports its
  checksum, its measured warm-up and every sample back to the parent.
  Re-exec start-up, source loading, checking, lowering, JIT
  compilation, Context construction and protocol I/O are all **outside**
  the reported duration.

- `--only <id>` must produce the same figure as the full suite for the
  same cell. That equivalence is the gate on process isolation, and it
  is what caught this.

- Only the **workload execution** is timed — not process start-up,
  compilation, JIT warm-up, Context creation, or I/O.
- One machine, one session; the runner records the machine (host, CPU, AC
  power) and each runtime's version.
- A subject whose runtime is absent is reported as `-` (not zero), never
  silently skipped.

## Workloads (10)

*(Rev 1, 2026-07-26: was "Workloads (8) — sqrt-free numeric set".
`Math` landed at P9, so the sqrt-free framing describes the original
eight rather than a standing constraint. They stay sqrt-free — the
checksums are frozen — and the ninth, `callbacks`, is added below.)*

Each workload's parameters are sized
so the C baseline runs in roughly 10–500 ms (a target, not a checked
invariant — `fib-recursive` measures about 3.6 ms on the arm64 dev
machine), and each produces an exact
**integer** checksum every subject must reproduce.

| Id | Workload | Integer checksum |
|---|---|---|
| fib-recursive | `fib(n)=fib(n-1)+fib(n-2)`, naive recursion, n=31 | `fib(31)` = 1346269 |
| fib-loop | iterative `fib` computed in a tight loop, accumulated (i32 wrap) | the accumulated sum (implementer pins n and loop count; same across subjects) |
| mandelbrot | escape-iteration count over an N×N grid, escape test `x²+y² ≥ 4` (no `sqrt`), cap 255 | sum of escape counts |
| primes | count primes up to N by trial division | the count |
| sort | quicksort (or heapsort) an `i32[]` of N elements seeded by a fixed LCG | sum of the sorted array (or a sampled set of indices) |
| tree | build and traverse balanced binary trees (allocate/free) to a fixed depth; subscript uses reference classes with explicit `unsafeDelete` | node-visit count / checksum |
| queen | count solutions to the N-queens problem | the solution count |
| particles | value-struct kinematics: M particles, K fixed-`dt` steps, `pos += vel*dt` (no `sqrt`) | an integer quantization of the final state (e.g. sum of `pos` cast to `i32`) |
| callbacks | array-callback pipeline over an `i32[]` of N elements seeded by the shared LCG, repeated K times: `map` → `filter` → `reduce`, **each callback taking the index** | the i32-wrapping accumulation of each round's `reduce` result |
| collect | build a live object graph of **mixed, deliberately unaligned sizes** — strings dominating — drop part of it, then reclaim | an i32 checksum over the surviving graph |

### `collect` — why it exists and what it is allowed to compare

*(Added Rev 2, 2026-07-26, by the P21 Phase Review.)* P21 took the
header word that held each classed block's exact payload size, so
`collect`'s mark phase now traces the **whole size-class capacity**
(`compiler.md` §21.2, `collisions.md` Q7). The cost is up to **3× the
words traced** — worst at a request just past a size-class boundary,
and `alloc_str` requests `8 + len`, so strings land there routinely.

Tracing padding is not merely reading more: the mark phase **pushes
every payload word onto a work list and looks each one up**, so the
extra words become extra work-list traffic and extra conservative
lookups, not just extra loads.

**Nothing measures it.** No workload calls `collect()`; `tree`
allocates and frees 131 071 nodes with `unsafeDelete`, exercising the
**allocator, not the collector**; `perf-gate`'s `a22` has no heap graph
at all. Correctness is well covered — three corpus entries call
`collect`, and P21's review ran a 200 000-operation randomized stress —
so this closes a **performance** gap only. It is the same shape as the
gap P18's review found for array callbacks.

**Sizes must be unaligned on purpose.** A workload built from
8-byte-aligned allocations measures the one case where P21 changed
nothing (ratio 1.0×). The point is the odd sizes.

**Subjects that cannot force a collection are reported `-`, with the
reason, and are not approximated.** C has no collector; its honest
analogue is freeing the graph explicitly, and that is what it does.
LuaJIT has `collectgarbage`. The JS runtimes need a flag to expose one
and may not have it here. The existing rule that a subject's timings
are withheld unless **every** subject's checksum matches applies to the
subjects that **ran**; a `-` subject contributes no checksum. Say in
the published row which subjects ran and which could not, because a
four-subject row read as a six-subject row is a misreading this
workload invites.

**What the comparison means, and does not.** subscript's `collect` is
explicit and conservative; the JS and Lua runtimes have generational or
incremental collectors. This is not "which GC is faster" — it is what
reclaiming one graph of N objects costs in each runtime's own idiom,
which is the same framing the `callbacks` row carries.

### `callbacks` — why it exists and how to read it

*(Added Rev 1, 2026-07-26, by the P18 Phase Review.)* Q27 made every
`T[]` and `FixedArray` callback method accept an index, which put a
per-element branch in `call_value` and `call_reduce` — the hot path of
the most-used stdlib surface. **No benchmark in the repository executed
any of it**: `perf-gate`'s only subject is `a22-matrix-propagation`,
which uses value structs and hand-written loops, and the eight
workloads above avoid array callbacks too — `sort` implements quicksort
by hand rather than calling `Array.sort`. A regression there was
invisible. This workload is what makes it visible, and a change in the
`subscript-ship` row for it is the regression signal.

**All arithmetic is i32 with wrapping**, each subject using its own
spelling (`int32_t` under `-fwrapv` in C, `| 0` in JS, `bit.tobit` in
LuaJIT, `i32` in subscript per C3). The LCG is the same fixed
`state = state*1664525 + 1013904223` that `sort` uses, so the input
array is identical across subjects.

**The subjects are deliberately not spelling this the same way, and
the ratio must be read accordingly.** C and Lua write loops, because
that is what those languages do; subscript and the JS runtimes call
`map`/`filter`/`reduce`, because that is what *those* do. So this
workload measures **what the idiomatic callback spelling costs against
a hand-written loop**, not a codegen deficit — the reporting rule
"report measured numbers, whatever they are" stands, and the published
row must carry this sentence so the number is not misread as the
others are read.

The implementer pins the exact N/M/K/loop counts and the precise checksum
formula per workload, identically across all six subjects, and records
them in `benchmarks/README.md`. The LCG (for `sort` and `callbacks`) is a fixed 32-bit
`state = state*1664525 + 1013904223`, shared by every subject so the
input array is identical.

## Layout

```
benchmarks/                        the subscript-benchmarks crate
  src/bin/cross-language.rs        the runner (bin `cross-language`)
  src/bin/perf-gate.rs             the P4 perf-gate harness (bin `perf-gate`)
  aot-entry.c                      AOT timing entry, shared by both bins
  a22-baseline.c                   the perf-gate's hand-written C baseline
  workloads/
    subscript/<id>.ts              one exported workload fn returning/printing the checksum
    c/<id>.c                       baseline + self-timing harness
    js/<id>.js                     run under both jsc and node
    lua/<id>.lua                   run under luajit
  results.json                     captured medians (per subject, per workload)
  README.md                        the generated table (C = 1.00×), with machine + versions
```

Re-run the cross-language suite with
`cargo run --offline --release -p subscript-benchmarks --bin cross-language`.

- The subscript workloads are ordinary accept-corpus-style programs
  (tsc-clean, decided spellings). They may be added to `corpus/accept/`
  with goldens if useful, or kept under `benchmarks/workloads/subscript/`; the
  implementer states which. Either way they type-check under stock `tsc`.
- The runner builds subscript-ship via the codegen crate's C emission and
  subscript-jit via `jit_bench`, shells out to `clang`/`luajit`/`jsc`/
  `node` for the others, verifies checksums agree, and writes
  `results.json` + `README.md`.

## Gate / done

- Every workload: all present subjects produce the identical integer
  checksum; timings recorded; the table renders with C = 1.00× and every
  other subject as a ratio + absolute median.
- Absent runtimes reported as `-`.
- `cargo build`/`cargo test` stay green (the benchmark runner does not
  break the workspace); reference sweep stays clean (no forbidden names).
