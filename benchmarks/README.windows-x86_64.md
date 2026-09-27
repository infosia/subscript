# Cross-language benchmarks — captured results

Snapshot captured 2026-09-27. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: x86_64 / windows
- CPU: Intel64 Family 6 Model 198 Stepping 2, GenuineIntel (20 logical cores)
- power: unknown

## Runtimes

- **C**: clang version 22.1.6 (https://github.com/llvm/llvm-project fc4aad7b5db3fff421df9a9637605b9ca5667881)
- **subscript**: subscript @ 246c7dd (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: absent
- **JSC**: absent
- **V8 (Node.js)**: Node.js v24.16.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (2.384 ms) | 1.12x (2.673 ms) | 1.67x (3.977 ms) | - | - | 2.95x (7.039 ms) |
| fib-loop | 973132000 | 1.00x (21.758 ms) | 0.96x (20.951 ms) | 2.27x (49.332 ms) | - | - | 1.54x (33.472 ms) |
| mandelbrot | 43027996 | 1.00x (75.514 ms) | 1.01x (76.410 ms) | 1.02x (76.863 ms) | - | - | 0.98x (73.765 ms) |
| primes | 41538 | 1.00x (31.792 ms) | 0.97x (30.970 ms) | 1.00x (31.651 ms) | - | - | 1.03x (32.610 ms) |
| sort | 3672124540 | 1.00x (13.830 ms) | 1.64x (22.614 ms) | 2.14x (29.604 ms) | - | - | 1.63x (22.525 ms) |
| tree | 3932130 | invalid (noise) | 102.618 ms | 577.766 ms | - | - | 40.829 ms |
| queen | 73712 | 1.00x (21.523 ms) | 1.09x (23.467 ms) | 1.61x (34.686 ms) | - | - | 1.42x (30.660 ms) |
| particles | 1712845248 | 1.00x (37.161 ms) | 2.04x (75.861 ms) | 6.80x (252.592 ms) | - | - | 2.98x (110.646 ms) |
| callbacks | -662567840 | 1.00x (13.261 ms) | 4.00x (53.090 ms) | 15.14x (200.822 ms) | - | - | 30.36x (402.534 ms) |
| collect | 1332546592 | 1.00x (37.639 ms) | 1.34x (50.278 ms) | 3.41x (128.255 ms) | - | - | 2.73x (102.611 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.203 s (81 iterations) | 0.202 s (73 iterations) | 0.202 s (48 iterations) | - | - | 0.200 s (27 iterations) |
| fib-loop | 0.200 s (9 iterations) | 0.215 s (10 iterations) | 0.248 s (5 iterations) | - | - | 0.212 s (6 iterations) |
| mandelbrot | 0.225 s (3 iterations) | 0.234 s (3 iterations) | 0.237 s (3 iterations) | - | - | 0.229 s (3 iterations) |
| primes | 0.231 s (7 iterations) | 0.223 s (7 iterations) | 0.204 s (6 iterations) | - | - | 0.215 s (7 iterations) |
| sort | 0.205 s (15 iterations) | 0.204 s (9 iterations) | 0.208 s (7 iterations) | - | - | 0.216 s (9 iterations) |
| tree | 0.445 s (3 iterations) | 0.309 s (3 iterations) | 1.834 s (3 iterations) | - | - | 0.204 s (4 iterations) |
| queen | 0.224 s (10 iterations) | 0.211 s (9 iterations) | 0.222 s (6 iterations) | - | - | 0.204 s (7 iterations) |
| particles | 0.221 s (6 iterations) | 0.228 s (3 iterations) | 0.781 s (3 iterations) | - | - | 0.323 s (3 iterations) |
| callbacks | 0.203 s (15 iterations) | 0.224 s (5 iterations) | 0.621 s (3 iterations) | - | - | 1.336 s (3 iterations) |
| collect | 0.234 s (6 iterations) | 0.212 s (4 iterations) | 0.433 s (3 iterations) | - | - | 0.338 s (3 iterations) |

**callbacks interpretation.** This workload measures what the idiomatic callback spelling costs against a hand-written loop, not a codegen deficit.

**collect interpretation.** This is not a cross-runtime “GC speed” claim; it compares reclaiming the pinned graph in each runtime's own explicit idiom. Ran: C, subscript-ship, subscript-jit, V8 (Node.js). Could not run: LuaJIT (lua runtime is not installed); JSC (JavaScriptCore is not installed). Failed: none.

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

Noise: interquartile range wider than 15% of the median (or invalid samples) for tree/C (IQR 16.1% of median) — those timings are invalid and withheld.
