# Cross-language benchmarks — captured results

Snapshot captured 2026-09-26. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.3.34.2)
- **subscript**: subscript @ 306f4f4 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.6.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.651 ms) | 1.02x (3.707 ms) | 2.19x (7.982 ms) | 1.96x (7.173 ms) | 1.49x (5.440 ms) | 2.66x (9.705 ms) |
| fib-loop | 973132000 | 1.00x (29.799 ms) | 1.02x (30.265 ms) | 2.40x (71.495 ms) | 1.46x (43.536 ms) | 1.08x (32.220 ms) | 1.56x (46.628 ms) |
| mandelbrot | 43027996 | 1.00x (125.941 ms) | 1.00x (125.516 ms) | 1.04x (130.523 ms) | 2.76x (347.100 ms) | 1.00x (125.440 ms) | 1.00x (125.814 ms) |
| primes | 41538 | 1.00x (21.819 ms) | 0.97x (21.189 ms) | 1.46x (31.921 ms) | 2.12x (46.331 ms) | 0.93x (20.340 ms) | 1.73x (37.646 ms) |
| sort | 3672124540 | 1.00x (15.432 ms) | 1.15x (17.781 ms) | 2.17x (33.443 ms) | 2.26x (34.810 ms) | 1.44x (22.160 ms) | 1.84x (28.448 ms) |
| tree | 3932130 | 1.00x (65.756 ms) | 2.01x (131.885 ms) | 6.31x (415.239 ms) | 2.26x (148.291 ms) | 0.33x (21.420 ms) | 0.48x (31.451 ms) |
| queen | 73712 | 1.00x (23.390 ms) | 1.10x (25.804 ms) | 1.53x (35.818 ms) | 1.37x (32.000 ms) | 1.24x (29.000 ms) | 1.78x (41.695 ms) |
| particles | 1712845248 | 1.00x (38.828 ms) | 1.92x (74.496 ms) | 12.02x (466.680 ms) | 3.84x (148.966 ms) | 1.90x (73.920 ms) | 3.59x (139.250 ms) |
| callbacks | -662567840 | 1.00x (13.103 ms) | 2.83x (37.132 ms) | 35.17x (460.855 ms) | 9.72x (127.297 ms) | 5.22x (68.400 ms) | 30.32x (397.241 ms) |
| collect | 1332546592 | 1.00x (32.494 ms) | 1.11x (36.216 ms) | 3.62x (117.707 ms) | 3.67x (119.263 ms) | 0.96x (31.340 ms) | 2.62x (85.107 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.204 s (47 iterations) | 0.203 s (47 iterations) | 0.207 s (26 iterations) | 0.200 s (28 iterations) | 0.204 s (37 iterations) | 0.203 s (21 iterations) |
| fib-loop | 0.200 s (6 iterations) | 0.223 s (7 iterations) | 0.223 s (3 iterations) | 0.218 s (5 iterations) | 0.224 s (7 iterations) | 0.234 s (5 iterations) |
| mandelbrot | 0.408 s (3 iterations) | 0.409 s (3 iterations) | 0.391 s (3 iterations) | 1.051 s (3 iterations) | 0.385 s (3 iterations) | 0.378 s (3 iterations) |
| primes | 0.208 s (8 iterations) | 0.203 s (8 iterations) | 0.224 s (7 iterations) | 0.229 s (5 iterations) | 0.205 s (10 iterations) | 0.225 s (6 iterations) |
| sort | 0.205 s (11 iterations) | 0.211 s (10 iterations) | 0.232 s (7 iterations) | 0.208 s (6 iterations) | 0.210 s (8 iterations) | 0.202 s (7 iterations) |
| tree | 0.230 s (3 iterations) | 0.422 s (3 iterations) | 1.249 s (3 iterations) | 0.450 s (3 iterations) | 0.212 s (9 iterations) | 0.206 s (6 iterations) |
| queen | 0.218 s (8 iterations) | 0.221 s (8 iterations) | 0.215 s (6 iterations) | 0.226 s (7 iterations) | 0.206 s (7 iterations) | 0.209 s (5 iterations) |
| particles | 0.227 s (5 iterations) | 0.255 s (3 iterations) | 1.421 s (3 iterations) | 0.446 s (3 iterations) | 0.258 s (3 iterations) | 0.434 s (3 iterations) |
| callbacks | 0.203 s (13 iterations) | 0.224 s (5 iterations) | 1.388 s (3 iterations) | 0.381 s (3 iterations) | 0.238 s (3 iterations) | 1.289 s (3 iterations) |
| collect | 0.232 s (6 iterations) | 0.213 s (5 iterations) | 0.363 s (3 iterations) | 0.364 s (3 iterations) | 0.223 s (7 iterations) | 0.299 s (3 iterations) |

**callbacks interpretation.** This workload measures what the idiomatic callback spelling costs against a hand-written loop, not a codegen deficit.

**collect interpretation.** This is not a cross-runtime “GC speed” claim; it compares reclaiming the pinned graph in each runtime's own explicit idiom. Ran: C, subscript-ship, subscript-jit, LuaJIT, JSC, V8 (Node.js). Could not run: none. Failed: none.

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + Context.free; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.
- **callbacks** — i32[1000000] from LCG state=state*1664525+1013904223 (seed 0x12345678), K=20 rounds; map(value,index)=(value+index) i32; filter(value,index)=((value^index)&3)!=0 (removes exactly 250000 elements per round); reduce(acc,value,index)=(acc+value+index) i32 from 0; checksum=checksum+round_result (i32 wrap)
- **collect** — N=20000 nodes x K=6 rounds from LCG state=state*1664525+1013904223 (seed 0x12345678); each 48-byte node owns unique strings of lengths 9/41/105/233 bytes (subscript requests 17/49/113/241 bytes, one byte past size-class payload capacities 16/48/112/240); keep exactly the nodes with (state&3)!=0 (15000 survivors/round), drop the rest, force collection (C: explicitly free), then traverse the surviving reverse-built chain; checksum per survivor in traversal order is checksum=(checksum*31+state+9+41+105+233) with i32 wrap; final checksum=1332546592

Noise: every recorded sample set is within +/-20% of its median.
