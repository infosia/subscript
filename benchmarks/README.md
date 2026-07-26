# Cross-language benchmarks — captured results

Snapshot captured 2026-07-26. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ c92e70b (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.637 ms) | 1.00x (3.632 ms) | 2.16x (7.856 ms) | 1.85x (6.731 ms) | 1.50x (5.440 ms) | 2.63x (9.579 ms) |
| fib-loop | 973132000 | 1.00x (29.298 ms) | 1.03x (30.162 ms) | 2.02x (59.277 ms) | 1.48x (43.474 ms) | 1.09x (31.980 ms) | 1.58x (46.329 ms) |
| mandelbrot | 43027996 | 1.00x (124.454 ms) | 1.00x (124.629 ms) | 1.05x (130.255 ms) | 2.77x (345.250 ms) | 1.00x (124.560 ms) | 1.00x (124.930 ms) |
| primes | 41538 | 1.00x (21.667 ms) | 0.96x (20.866 ms) | 1.46x (31.611 ms) | 2.11x (45.611 ms) | 0.93x (20.060 ms) | 1.71x (37.100 ms) |
| sort | 3672124540 | 1.00x (15.265 ms) | 1.21x (18.519 ms) | 4.77x (72.853 ms) | 2.27x (34.719 ms) | 1.47x (22.460 ms) | 1.82x (27.828 ms) |
| tree | 3932130 | 1.00x (65.128 ms) | 1.69x (110.259 ms) | 10.16x (661.587 ms) | 2.21x (143.995 ms) | 0.33x (21.300 ms) | 0.47x (30.420 ms) |
| queen | 73712 | 1.00x (23.591 ms) | 1.04x (24.609 ms) | 1.49x (35.042 ms) | 1.51x (35.567 ms) | 1.22x (28.880 ms) | 1.77x (41.703 ms) |
| particles | 1712845248 | 1.00x (38.648 ms) | 1.94x (74.850 ms) | 14.46x (559.005 ms) | 3.83x (148.201 ms) | 1.90x (73.600 ms) | 3.58x (138.357 ms) |
| callbacks | -662567840 | 1.00x (13.033 ms) | 22.91x (298.530 ms) | 24.47x (318.884 ms) | 9.17x (119.500 ms) | 5.23x (68.140 ms) | 29.71x (387.168 ms) |
| collect | 1332546592 | 1.00x (32.569 ms) | 6.43x (209.374 ms) | 14.21x (462.749 ms) | 3.70x (120.434 ms) | 1.05x (34.300 ms) | 2.62x (85.257 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.202 s (47 iterations) | 0.203 s (47 iterations) | 0.205 s (26 iterations) | 0.202 s (30 iterations) | 0.201 s (37 iterations) | 0.201 s (21 iterations) |
| fib-loop | 0.207 s (6 iterations) | 0.211 s (6 iterations) | 0.234 s (4 iterations) | 0.218 s (5 iterations) | 0.223 s (7 iterations) | 0.233 s (5 iterations) |
| mandelbrot | 0.405 s (3 iterations) | 0.405 s (3 iterations) | 0.392 s (3 iterations) | 1.036 s (3 iterations) | 0.376 s (3 iterations) | 0.377 s (3 iterations) |
| primes | 0.209 s (8 iterations) | 0.218 s (9 iterations) | 0.221 s (7 iterations) | 0.228 s (5 iterations) | 0.202 s (10 iterations) | 0.223 s (6 iterations) |
| sort | 0.200 s (11 iterations) | 0.216 s (10 iterations) | 0.218 s (3 iterations) | 0.208 s (6 iterations) | 0.222 s (9 iterations) | 0.205 s (7 iterations) |
| tree | 0.227 s (3 iterations) | 0.356 s (3 iterations) | 2.058 s (3 iterations) | 0.436 s (3 iterations) | 0.218 s (9 iterations) | 0.200 s (6 iterations) |
| queen | 0.219 s (8 iterations) | 0.201 s (7 iterations) | 0.210 s (6 iterations) | 0.214 s (6 iterations) | 0.205 s (7 iterations) | 0.209 s (5 iterations) |
| particles | 0.225 s (5 iterations) | 0.249 s (3 iterations) | 1.677 s (3 iterations) | 0.448 s (3 iterations) | 0.252 s (3 iterations) | 0.418 s (3 iterations) |
| callbacks | 0.211 s (14 iterations) | 0.929 s (3 iterations) | 0.967 s (3 iterations) | 0.394 s (3 iterations) | 0.229 s (3 iterations) | 1.258 s (3 iterations) |
| collect | 0.202 s (5 iterations) | 0.668 s (3 iterations) | 1.408 s (3 iterations) | 0.366 s (3 iterations) | 0.204 s (6 iterations) | 0.289 s (3 iterations) |

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

Noise: every recorded sample set is within +/-20% of its median.
