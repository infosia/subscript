# Cross-language benchmarks — captured results

Snapshot captured 2026-07-27. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 79b2a97 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.656 ms) | 1.02x (3.722 ms) | 2.21x (8.067 ms) | 2.15x (7.860 ms) | 1.49x (5.440 ms) | 2.63x (9.628 ms) |
| fib-loop | 973132000 | 1.00x (29.421 ms) | 1.03x (30.421 ms) | 2.03x (59.748 ms) | 1.52x (44.679 ms) | 1.10x (32.300 ms) | 1.63x (47.854 ms) |
| mandelbrot | 43027996 | 1.00x (125.106 ms) | 1.00x (125.396 ms) | 1.05x (131.342 ms) | 2.85x (356.073 ms) | 1.00x (125.400 ms) | 1.01x (126.875 ms) |
| primes | 41538 | 1.00x (21.969 ms) | 0.96x (21.102 ms) | 1.45x (31.900 ms) | 2.13x (46.840 ms) | 0.94x (20.660 ms) | 1.75x (38.394 ms) |
| sort | 3672124540 | 1.00x (15.880 ms) | 1.21x (19.185 ms) | 5.02x (79.645 ms) | 2.21x (35.095 ms) | 1.45x (23.020 ms) | 1.77x (28.067 ms) |
| tree | 3932130 | 1.00x (65.647 ms) | 1.43x (93.759 ms) | 7.81x (512.754 ms) | 2.24x (146.871 ms) | 0.32x (20.880 ms) | 0.47x (31.019 ms) |
| queen | 73712 | 1.00x (23.721 ms) | 1.04x (24.752 ms) | 1.48x (34.995 ms) | 1.55x (36.739 ms) | 1.23x (29.060 ms) | 1.76x (41.834 ms) |
| particles | 1712845248 | 1.00x (38.825 ms) | 1.93x (75.093 ms) | 16.07x (623.914 ms) | 3.84x (149.268 ms) | 1.91x (74.040 ms) | 3.58x (139.185 ms) |
| callbacks | -662567840 | 1.00x (13.064 ms) | 23.05x (301.097 ms) | 25.92x (338.672 ms) | 9.10x (118.879 ms) | 5.24x (68.400 ms) | 29.69x (387.819 ms) |
| collect | 1332546592 | 1.00x (34.998 ms) | 6.12x (214.083 ms) | 6.19x (216.505 ms) | 3.81x (133.264 ms) | 0.98x (34.420 ms) | 2.50x (87.608 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.200 s (46 iterations) | 0.203 s (48 iterations) | 0.202 s (25 iterations) | 0.205 s (26 iterations) | 0.203 s (37 iterations) | 0.203 s (21 iterations) |
| fib-loop | 0.202 s (6 iterations) | 0.206 s (6 iterations) | 0.237 s (4 iterations) | 0.222 s (5 iterations) | 0.230 s (7 iterations) | 0.241 s (5 iterations) |
| mandelbrot | 0.409 s (3 iterations) | 0.409 s (3 iterations) | 0.395 s (3 iterations) | 1.067 s (3 iterations) | 0.379 s (3 iterations) | 0.383 s (3 iterations) |
| primes | 0.212 s (8 iterations) | 0.203 s (9 iterations) | 0.225 s (7 iterations) | 0.231 s (5 iterations) | 0.206 s (10 iterations) | 0.228 s (6 iterations) |
| sort | 0.214 s (12 iterations) | 0.207 s (10 iterations) | 0.239 s (3 iterations) | 0.211 s (6 iterations) | 0.205 s (8 iterations) | 0.207 s (7 iterations) |
| tree | 0.222 s (3 iterations) | 0.312 s (3 iterations) | 1.552 s (3 iterations) | 0.441 s (3 iterations) | 0.205 s (9 iterations) | 0.229 s (7 iterations) |
| queen | 0.223 s (9 iterations) | 0.223 s (8 iterations) | 0.211 s (6 iterations) | 0.220 s (6 iterations) | 0.205 s (7 iterations) | 0.211 s (5 iterations) |
| particles | 0.217 s (5 iterations) | 0.256 s (3 iterations) | 1.871 s (3 iterations) | 0.448 s (3 iterations) | 0.255 s (3 iterations) | 0.422 s (3 iterations) |
| callbacks | 0.201 s (13 iterations) | 0.932 s (3 iterations) | 1.024 s (3 iterations) | 0.361 s (3 iterations) | 0.233 s (3 iterations) | 1.258 s (3 iterations) |
| collect | 0.203 s (5 iterations) | 0.673 s (3 iterations) | 0.665 s (3 iterations) | 0.399 s (3 iterations) | 0.208 s (6 iterations) | 0.306 s (3 iterations) |

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
