# Cross-language benchmarks — captured results

Snapshot captured 2026-08-27. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 74a091c (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.635 ms) | 1.00x (3.622 ms) | 2.17x (7.890 ms) | 1.87x (6.787 ms) | 1.49x (5.420 ms) | 2.63x (9.567 ms) |
| fib-loop | 973132000 | 1.00x (29.254 ms) | 1.03x (30.048 ms) | 2.41x (70.455 ms) | 1.48x (43.277 ms) | 1.09x (32.000 ms) | 1.58x (46.347 ms) |
| mandelbrot | 43027996 | 1.00x (124.075 ms) | 1.00x (124.547 ms) | 1.04x (129.508 ms) | 2.78x (344.636 ms) | 1.00x (124.520 ms) | 1.01x (125.124 ms) |
| primes | 41538 | 1.00x (21.714 ms) | 1.00x (21.615 ms) | 1.47x (31.815 ms) | 2.10x (45.693 ms) | 0.92x (20.080 ms) | 1.71x (37.155 ms) |
| sort | 3672124540 | 1.00x (15.292 ms) | 1.24x (18.995 ms) | 2.33x (35.701 ms) | 2.29x (34.971 ms) | 1.45x (22.220 ms) | 1.83x (27.995 ms) |
| tree | 3932130 | 1.00x (65.247 ms) | 1.54x (100.575 ms) | 6.20x (404.494 ms) | 2.19x (142.785 ms) | 0.32x (21.040 ms) | 0.47x (30.791 ms) |
| queen | 73712 | 1.00x (23.630 ms) | 1.09x (25.670 ms) | 1.51x (35.654 ms) | 1.36x (32.125 ms) | 1.23x (28.980 ms) | 1.76x (41.666 ms) |
| particles | 1712845248 | 1.00x (38.677 ms) | 2.12x (81.950 ms) | 12.75x (492.986 ms) | 3.93x (152.177 ms) | 1.95x (75.520 ms) | 3.67x (142.111 ms) |
| callbacks | -662567840 | 1.00x (13.365 ms) | 22.95x (306.732 ms) | 26.35x (352.205 ms) | 9.58x (128.081 ms) | 5.25x (70.220 ms) | 29.76x (397.806 ms) |
| collect | 1332546592 | 1.00x (32.348 ms) | 6.45x (208.611 ms) | 7.04x (227.691 ms) | 3.68x (119.013 ms) | 1.04x (33.580 ms) | 2.60x (83.962 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.203 s (47 iterations) | 0.201 s (47 iterations) | 0.205 s (26 iterations) | 0.204 s (30 iterations) | 0.201 s (37 iterations) | 0.201 s (21 iterations) |
| fib-loop | 0.207 s (6 iterations) | 0.213 s (6 iterations) | 0.212 s (3 iterations) | 0.217 s (5 iterations) | 0.223 s (7 iterations) | 0.233 s (5 iterations) |
| mandelbrot | 0.402 s (3 iterations) | 0.405 s (3 iterations) | 0.388 s (3 iterations) | 1.035 s (3 iterations) | 0.376 s (3 iterations) | 0.377 s (3 iterations) |
| primes | 0.209 s (8 iterations) | 0.205 s (8 iterations) | 0.223 s (7 iterations) | 0.228 s (5 iterations) | 0.203 s (10 iterations) | 0.224 s (6 iterations) |
| sort | 0.213 s (12 iterations) | 0.204 s (9 iterations) | 0.211 s (6 iterations) | 0.209 s (6 iterations) | 0.202 s (8 iterations) | 0.203 s (7 iterations) |
| tree | 0.220 s (3 iterations) | 0.331 s (3 iterations) | 1.212 s (3 iterations) | 0.431 s (3 iterations) | 0.209 s (9 iterations) | 0.230 s (7 iterations) |
| queen | 0.219 s (8 iterations) | 0.208 s (7 iterations) | 0.214 s (6 iterations) | 0.226 s (7 iterations) | 0.204 s (7 iterations) | 0.209 s (5 iterations) |
| particles | 0.226 s (5 iterations) | 0.276 s (3 iterations) | 1.467 s (3 iterations) | 0.453 s (3 iterations) | 0.260 s (3 iterations) | 0.435 s (3 iterations) |
| callbacks | 0.206 s (14 iterations) | 0.961 s (3 iterations) | 1.068 s (3 iterations) | 0.382 s (3 iterations) | 0.243 s (3 iterations) | 1.301 s (3 iterations) |
| collect | 0.219 s (6 iterations) | 0.656 s (3 iterations) | 0.693 s (3 iterations) | 0.361 s (3 iterations) | 0.206 s (6 iterations) | 0.286 s (3 iterations) |

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
