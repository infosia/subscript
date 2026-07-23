# Cross-language benchmarks — contract

Status: Rev 0, 2026-07-23. A cross-language performance comparison of the
subscript ship and dev tiers against a C baseline and JIT-enabled
scripting runtimes. Not a gate (the P4 gate in `compiler.md` §3/§9 is the
gate); this is a published comparison. Lives in `benchmarks/`.

## Purpose and honesty rules

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
| C | hand-written C, `clang -O2 -ffp-contract=off` | self-timed (monotonic clock), prints median |
| subscript-ship | HIR → C → `clang -std=c11 -O2 -fwrapv -ffp-contract=off` (the ship tier, `compiler.md` §11) | externally timed by the runner (AOT entry loops the exported workload fn) |
| subscript-jit | HIR → Cranelift JIT (the dev tier) | externally timed by the runner (`jit_bench`) |
| LuaJIT | [luajit.org](https://luajit.org) 2.1 | self-timed (`os.clock`), prints median |
| JSC | JavaScriptCore, JIT-enabled ([webkit.org](https://webkit.org)) | self-timed (`Date.now`/`performance.now`), prints median |
| V8 | Node.js ([nodejs.org](https://nodejs.org)) | self-timed, prints median |

subscript has no clock primitive, so its two tiers are timed by the
runner, not from inside the script — the C/JS/Lua subjects time
themselves and print `<checksum> <median_seconds>`.

## Timing methodology (mirrors `compiler.md` §9)

- Each subject: **≥3 warm-up runs discarded, ≥11 timed runs, report the
  median**; also record min/max. A spread wider than ±20% of the median
  invalidates the run (machine too noisy) and it is redone with more
  warm-up.
- Only the **workload execution** is timed — not process start-up,
  compilation, JIT warm-up, Context creation, or I/O.
- One machine, one session; the runner records the machine (host, CPU, AC
  power) and each runtime's version.
- A subject whose runtime is absent is reported as `-` (not zero), never
  silently skipped.

## Workloads (8) — sqrt-free numeric set

subscript's current surface has no math library (no `sqrt`/trig), so the
set avoids transcendental functions. Each workload's parameters are sized
so the C baseline runs in roughly 10–500 ms, and each produces an exact
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

The implementer pins the exact N/M/K/loop counts and the precise checksum
formula per workload, identically across all six subjects, and records
them in `benchmarks/README.md`. The LCG (for `sort`) is a fixed 32-bit
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
