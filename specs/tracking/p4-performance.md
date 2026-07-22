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

## Owner decision (2026-07-23)

The lowering is optimized and the gate re-measured before the backend
decision is judged (`specs/blocks/compiler.md` §10, P4.1). Rationale:
the profile and the lowering inspection place the dominant cost in this
project's code generation, so switching backend now would answer a
question the measurement has not asked. §3's thresholds and §9's
methodology are unchanged; the standing gate protects correctness while
the optimization lands.

## Artifacts

`bench/a22-baseline.c` (baseline, header comment names the corpus
entry), `bench/aot-entry.c`, `bench/src/main.rs` (harness crate
`subscript-bench`, bin `bench`). Run:
`cargo run --offline --release -p subscript-bench --bin bench --
--warmup 30 --timed 11`. Release is enforced. No build products are
written inside the repository.
