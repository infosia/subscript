# Cross-language benchmarks — captured results

Snapshot captured 2026-07-26. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 60e2592 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs uses the same schedule: 60 warm-up runs discarded, 15 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for every measured subject. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.683 ms) | 1.00x (3.666 ms) | 2.21x (8.132 ms) | 2.00x (7.375 ms) | 1.49x (5.480 ms) | 2.62x (9.656 ms) |
| fib-loop | 973132000 | invalid (noise) | 30.420 ms | 59.251 ms | 43.613 ms | 32.380 ms | 30.115 ms |
| mandelbrot | 43027996 | invalid (noise) | 124.814 ms | 130.580 ms | 347.277 ms | 125.040 ms | 125.739 ms |
| primes | 41538 | invalid (noise) | 20.947 ms | 31.819 ms | 45.641 ms | 20.160 ms | 37.231 ms |
| sort | 3672124540 | 1.00x (15.342 ms) | 1.24x (19.077 ms) | 4.77x (73.192 ms) | 2.28x (34.986 ms) | 1.49x (22.820 ms) | 1.83x (28.067 ms) |
| tree | 3932130 | 1.00x (65.559 ms) | 1.69x (110.748 ms) | 10.27x (673.183 ms) | 2.18x (142.869 ms) | 0.31x (20.060 ms) | 0.46x (30.236 ms) |
| queen | 73712 | 1.00x (23.760 ms) | 1.04x (24.680 ms) | 1.48x (35.162 ms) | 1.47x (34.915 ms) | 1.22x (29.020 ms) | 1.76x (41.851 ms) |
| particles | 1712845248 | 1.00x (38.784 ms) | 1.93x (74.770 ms) | 14.47x (561.275 ms) | 3.84x (149.012 ms) | 1.90x (73.840 ms) | 3.59x (139.416 ms) |
| callbacks | -662567840 | 1.00x (13.119 ms) | 22.86x (299.853 ms) | 24.40x (320.141 ms) | 9.14x (119.887 ms) | 5.09x (66.780 ms) | 29.61x (388.476 ms) |
| collect | 1332546592 | 1.00x (32.463 ms) | 6.44x (208.932 ms) | 6.88x (223.268 ms) | 3.70x (120.104 ms) | invalid (noise) | 2.61x (84.620 ms) |

**callbacks interpretation.** This workload measures what the idiomatic callback spelling costs against a hand-written loop, not a codegen deficit.

**collect interpretation.** This is not a cross-runtime “GC speed” claim; it compares reclaiming the pinned graph in each runtime's own explicit idiom. Ran: C, subscript-ship, subscript-jit, LuaJIT, JSC, V8 (Node.js). Could not run: none. Failed: none.

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.
- **callbacks** — i32[1000000] from LCG state=state*1664525+1013904223 (seed 0x12345678), K=20 rounds; map(value,index)=(value+index) i32; filter(value,index)=((value^index)&3)!=0 (removes exactly 250000 elements per round); reduce(acc,value,index)=(acc+value+index) i32 from 0; checksum=checksum+round_result (i32 wrap)
- **collect** — N=20000 nodes x K=6 rounds from LCG state=state*1664525+1013904223 (seed 0x12345678); each 48-byte node owns unique strings of lengths 9/41/105/233 bytes (subscript requests 17/49/113/241 bytes, one byte past size-class payload capacities 16/48/112/240); keep exactly the nodes with (state&3)!=0 (15000 survivors/round), drop the rest, force collection (C: explicitly free), then traverse the surviving reverse-built chain; checksum per survivor in traversal order is checksum=(checksum*31+state+9+41+105+233) with i32 wrap; final checksum=1332546592

Noise: wider than +/-20% spread for fib-loop/C (86%), mandelbrot/C (27%), primes/C (52%), collect/JSC (24%) — those timings are invalid and withheld.
