# Cross-language benchmarks — captured results

Snapshot captured 2026-08-29. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ bbb9d78 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.641 ms) | 1.00x (3.628 ms) | 2.17x (7.906 ms) | 1.88x (6.857 ms) | 1.49x (5.420 ms) | 2.63x (9.578 ms) |
| fib-loop | 973132000 | 1.00x (29.296 ms) | 1.04x (30.387 ms) | 2.42x (70.996 ms) | 1.48x (43.377 ms) | 1.09x (32.020 ms) | 1.58x (46.395 ms) |
| mandelbrot | 43027996 | 1.00x (124.347 ms) | 1.00x (124.695 ms) | 1.04x (129.658 ms) | 2.78x (345.282 ms) | 1.00x (124.720 ms) | 1.01x (125.343 ms) |
| primes | 41538 | 1.00x (21.722 ms) | 1.00x (21.662 ms) | 1.47x (31.873 ms) | 2.11x (45.766 ms) | 0.93x (20.120 ms) | 1.71x (37.201 ms) |
| sort | 3672124540 | 1.00x (15.319 ms) | 1.24x (19.066 ms) | 2.26x (34.645 ms) | 2.26x (34.660 ms) | 1.45x (22.220 ms) | 1.82x (27.826 ms) |
| tree | 3932130 | 1.00x (65.365 ms) | 1.55x (101.398 ms) | 6.23x (407.444 ms) | 2.23x (145.555 ms) | 0.32x (21.160 ms) | 0.47x (30.750 ms) |
| queen | 73712 | 1.00x (23.635 ms) | 1.09x (25.651 ms) | 1.50x (35.434 ms) | 1.46x (34.523 ms) | 1.23x (28.980 ms) | 1.77x (41.730 ms) |
| particles | 1712845248 | 1.00x (38.725 ms) | 2.12x (82.157 ms) | 12.13x (469.891 ms) | 3.84x (148.691 ms) | 1.91x (73.820 ms) | 3.58x (138.817 ms) |
| callbacks | -662567840 | 1.00x (13.067 ms) | 21.84x (285.370 ms) | 24.41x (318.967 ms) | 9.51x (124.258 ms) | 5.34x (69.800 ms) | 29.73x (388.434 ms) |
| collect | 1332546592 | 1.00x (32.414 ms) | 1.00x (32.278 ms) | 3.20x (103.609 ms) | 3.69x (119.450 ms) | 1.04x (33.580 ms) | invalid (noise) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.200 s (46 iterations) | 0.201 s (47 iterations) | 0.206 s (26 iterations) | 0.206 s (30 iterations) | 0.202 s (37 iterations) | 0.201 s (21 iterations) |
| fib-loop | 0.207 s (6 iterations) | 0.213 s (6 iterations) | 0.214 s (3 iterations) | 0.217 s (5 iterations) | 0.223 s (7 iterations) | 0.233 s (5 iterations) |
| mandelbrot | 0.402 s (3 iterations) | 0.405 s (3 iterations) | 0.389 s (3 iterations) | 1.037 s (3 iterations) | 0.377 s (3 iterations) | 0.377 s (3 iterations) |
| primes | 0.208 s (8 iterations) | 0.208 s (8 iterations) | 0.223 s (7 iterations) | 0.228 s (5 iterations) | 0.202 s (10 iterations) | 0.224 s (6 iterations) |
| sort | 0.213 s (12 iterations) | 0.203 s (9 iterations) | 0.208 s (6 iterations) | 0.208 s (6 iterations) | 0.220 s (9 iterations) | 0.202 s (7 iterations) |
| tree | 0.227 s (3 iterations) | 0.333 s (3 iterations) | 1.218 s (3 iterations) | 0.443 s (3 iterations) | 0.220 s (9 iterations) | 0.207 s (6 iterations) |
| queen | 0.216 s (8 iterations) | 0.203 s (7 iterations) | 0.213 s (6 iterations) | 0.208 s (6 iterations) | 0.204 s (7 iterations) | 0.210 s (5 iterations) |
| particles | 0.228 s (5 iterations) | 0.274 s (3 iterations) | 1.407 s (3 iterations) | 0.446 s (3 iterations) | 0.253 s (3 iterations) | 0.420 s (3 iterations) |
| callbacks | 0.201 s (13 iterations) | 0.894 s (3 iterations) | 0.966 s (3 iterations) | 0.370 s (3 iterations) | 0.236 s (3 iterations) | 1.280 s (3 iterations) |
| collect | 0.231 s (6 iterations) | 0.223 s (6 iterations) | 0.320 s (3 iterations) | 0.361 s (3 iterations) | 0.205 s (6 iterations) | 0.293 s (3 iterations) |

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

Noise: wider than +/-20% spread for collect/V8 (Node.js) (33%) — those timings are invalid and withheld.
