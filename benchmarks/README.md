# Cross-language benchmarks — captured results

Snapshot captured 2026-08-29. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 2335523 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.108 ms) | 0.97x (3.978 ms) | 2.16x (8.881 ms) | 1.77x (7.286 ms) | 1.44x (5.920 ms) | 2.48x (10.171 ms) |
| fib-loop | 973132000 | 1.00x (32.550 ms) | 1.01x (32.852 ms) | 2.44x (79.497 ms) | 1.44x (46.898 ms) | 1.06x (34.660 ms) | 1.55x (50.295 ms) |
| mandelbrot | 43027996 | 1.00x (138.455 ms) | 0.98x (136.329 ms) | 1.02x (141.264 ms) | 2.80x (387.609 ms) | 0.98x (135.600 ms) | 1.01x (139.438 ms) |
| primes | 41538 | 1.00x (23.645 ms) | 1.02x (24.006 ms) | 1.44x (33.994 ms) | 2.10x (49.586 ms) | 0.92x (21.760 ms) | 1.76x (41.602 ms) |
| sort | 3672124540 | 1.00x (17.115 ms) | 1.12x (19.248 ms) | 2.15x (36.838 ms) | 2.17x (37.128 ms) | 1.44x (24.600 ms) | 1.74x (29.719 ms) |
| tree | 3932130 | 1.00x (69.473 ms) | 1.55x (107.629 ms) | 6.27x (435.707 ms) | 2.26x (156.950 ms) | 0.33x (23.260 ms) | 0.46x (32.235 ms) |
| queen | 73712 | 1.00x (25.138 ms) | 1.12x (28.184 ms) | 1.51x (37.924 ms) | 1.51x (37.918 ms) | 1.23x (30.860 ms) | 1.77x (44.403 ms) |
| particles | 1712845248 | 1.00x (41.392 ms) | 2.18x (90.266 ms) | 12.02x (497.350 ms) | 3.95x (163.456 ms) | 1.90x (78.580 ms) | 3.68x (152.485 ms) |
| callbacks | -662567840 | 1.00x (14.106 ms) | 2.88x (40.671 ms) | 21.21x (299.207 ms) | 8.95x (126.186 ms) | 4.89x (69.040 ms) | 29.88x (421.486 ms) |
| collect | 1332546592 | 1.00x (35.910 ms) | 0.99x (35.549 ms) | 3.21x (115.157 ms) | 3.71x (133.240 ms) | 1.01x (36.440 ms) | 2.59x (93.009 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.200 s (48 iterations) | 0.202 s (49 iterations) | 0.201 s (23 iterations) | 0.205 s (28 iterations) | 0.201 s (34 iterations) | 0.209 s (20 iterations) |
| fib-loop | 0.232 s (7 iterations) | 0.201 s (6 iterations) | 0.240 s (3 iterations) | 0.236 s (5 iterations) | 0.207 s (6 iterations) | 0.202 s (4 iterations) |
| mandelbrot | 0.421 s (3 iterations) | 0.407 s (3 iterations) | 0.433 s (3 iterations) | 1.129 s (3 iterations) | 0.410 s (3 iterations) | 0.408 s (3 iterations) |
| primes | 0.222 s (9 iterations) | 0.221 s (9 iterations) | 0.226 s (7 iterations) | 0.247 s (5 iterations) | 0.219 s (10 iterations) | 0.208 s (5 iterations) |
| sort | 0.208 s (12 iterations) | 0.213 s (11 iterations) | 0.221 s (6 iterations) | 0.223 s (6 iterations) | 0.217 s (8 iterations) | 0.223 s (7 iterations) |
| tree | 0.220 s (3 iterations) | 0.326 s (3 iterations) | 1.333 s (3 iterations) | 0.475 s (3 iterations) | 0.202 s (8 iterations) | 0.215 s (6 iterations) |
| queen | 0.204 s (8 iterations) | 0.225 s (8 iterations) | 0.229 s (6 iterations) | 0.227 s (6 iterations) | 0.217 s (7 iterations) | 0.222 s (5 iterations) |
| particles | 0.217 s (5 iterations) | 0.275 s (3 iterations) | 1.508 s (3 iterations) | 0.474 s (3 iterations) | 0.269 s (3 iterations) | 0.451 s (3 iterations) |
| callbacks | 0.206 s (14 iterations) | 0.214 s (5 iterations) | 0.921 s (3 iterations) | 0.379 s (3 iterations) | 0.228 s (3 iterations) | 1.259 s (3 iterations) |
| collect | 0.227 s (6 iterations) | 0.215 s (6 iterations) | 0.353 s (3 iterations) | 0.401 s (3 iterations) | 0.218 s (6 iterations) | 0.318 s (3 iterations) |

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
