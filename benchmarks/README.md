# Cross-language benchmarks — captured results

Snapshot captured 2026-11-21. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: clang: Apple clang version 21.0.0 (clang-2100.4.1.1); C [-O2 -fwrapv -ffp-contract=off baseline.c -o subject.exe]; ship [-std=c11 -O2 -fwrapv -ffp-contract=off -fwrapv program.c entry.c runtime-staticlib -Wl,-dead_strip -o subject.exe]
- **subscript**: subscript @ 072bfb54 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 27.2.1)
- **V8 (Node.js)**: Node.js v24.22.1

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.636 ms) | 1.02x (3.726 ms) | 2.23x (8.121 ms) | 1.94x (7.046 ms) | 1.46x (5.320 ms) | 2.63x (9.569 ms) |
| fib-loop | 973132000 | 1.00x (29.295 ms) | 1.03x (30.246 ms) | 2.44x (71.347 ms) | 1.48x (43.329 ms) | 1.10x (32.140 ms) | 1.58x (46.380 ms) |
| mandelbrot | 43027996 | 1.00x (124.220 ms) | 1.00x (124.502 ms) | 1.04x (129.541 ms) | 2.78x (345.673 ms) | 1.00x (124.620 ms) | 1.01x (125.205 ms) |
| primes | 41538 | 1.00x (21.761 ms) | 0.95x (20.757 ms) | 1.46x (31.801 ms) | 2.10x (45.765 ms) | 0.92x (20.120 ms) | 1.71x (37.264 ms) |
| sort | 3672124540 | 1.00x (15.324 ms) | 1.16x (17.790 ms) | 2.25x (34.495 ms) | 2.28x (34.884 ms) | 1.48x (22.740 ms) | 1.83x (27.980 ms) |
| tree | 3932130 | 1.00x (57.379 ms) | 1.78x (102.296 ms) | 7.22x (414.448 ms) | 2.48x (142.540 ms) | 0.36x (20.760 ms) | 0.53x (30.429 ms) |
| queen | 73712 | 1.00x (23.672 ms) | 1.08x (25.675 ms) | 1.51x (35.798 ms) | 1.50x (35.529 ms) | 1.30x (30.760 ms) | 1.76x (41.700 ms) |
| particles | 1712845248 | 1.00x (38.792 ms) | 1.92x (74.405 ms) | 11.40x (442.412 ms) | 3.83x (148.460 ms) | 1.90x (73.800 ms) | 3.58x (138.710 ms) |
| callbacks | -662567840 | 1.00x (13.057 ms) | 2.79x (36.404 ms) | 19.58x (255.607 ms) | 9.09x (118.701 ms) | 4.81x (62.820 ms) | 29.65x (387.122 ms) |
| collect | 1332546592 | 1.00x (31.323 ms) | 1.15x (36.020 ms) | 3.64x (113.908 ms) | 3.79x (118.624 ms) | 1.13x (35.260 ms) | 2.78x (86.954 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.202 s (53 iterations) | 0.201 s (52 iterations) | 0.203 s (25 iterations) | 0.205 s (29 iterations) | 0.203 s (38 iterations) | 0.201 s (21 iterations) |
| fib-loop | 0.214 s (7 iterations) | 0.221 s (7 iterations) | 0.214 s (3 iterations) | 0.217 s (5 iterations) | 0.215 s (7 iterations) | 0.233 s (5 iterations) |
| mandelbrot | 0.383 s (3 iterations) | 0.382 s (3 iterations) | 0.389 s (3 iterations) | 1.036 s (3 iterations) | 0.376 s (3 iterations) | 0.378 s (3 iterations) |
| primes | 0.208 s (9 iterations) | 0.219 s (10 iterations) | 0.222 s (7 iterations) | 0.228 s (5 iterations) | 0.202 s (10 iterations) | 0.224 s (6 iterations) |
| sort | 0.207 s (13 iterations) | 0.204 s (11 iterations) | 0.207 s (6 iterations) | 0.210 s (6 iterations) | 0.201 s (8 iterations) | 0.206 s (7 iterations) |
| tree | 0.239 s (4 iterations) | 0.316 s (3 iterations) | 1.245 s (3 iterations) | 0.432 s (3 iterations) | 0.200 s (9 iterations) | 0.230 s (7 iterations) |
| queen | 0.219 s (9 iterations) | 0.218 s (8 iterations) | 0.215 s (6 iterations) | 0.214 s (6 iterations) | 0.216 s (7 iterations) | 0.209 s (5 iterations) |
| particles | 0.201 s (5 iterations) | 0.232 s (3 iterations) | 1.325 s (3 iterations) | 0.446 s (3 iterations) | 0.253 s (3 iterations) | 0.420 s (3 iterations) |
| callbacks | 0.203 s (15 iterations) | 0.229 s (6 iterations) | 0.768 s (3 iterations) | 0.355 s (3 iterations) | 0.207 s (3 iterations) | 1.251 s (3 iterations) |
| collect | 0.204 s (6 iterations) | 0.233 s (6 iterations) | 0.346 s (3 iterations) | 0.361 s (3 iterations) | 0.212 s (6 iterations) | 0.283 s (3 iterations) |

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

Noise: no recorded sample set has an interquartile range wider than 15% of the median.
