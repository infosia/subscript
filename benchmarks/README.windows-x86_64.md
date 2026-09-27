# Cross-language benchmarks — captured results

Snapshot captured 2026-09-27. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: x86_64 / windows
- CPU: Intel64 Family 6 Model 198 Stepping 2, GenuineIntel (20 logical cores)
- power: unknown

## Runtimes

- **C**: clang version 22.1.6 (https://github.com/llvm/llvm-project fc4aad7b5db3fff421df9a9637605b9ca5667881)
- **subscript**: subscript @ 514ab82 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: absent
- **JSC**: absent
- **V8 (Node.js)**: Node.js v24.16.0

## Method

Every subject that runs discards at least 3 warm-up iterations and continues until measured workload execution reaches the 200 ms floor, then performs 11 timed runs and reports the median. `--warmup` is the minimum iteration count; the time floor is always additional. The runner rejects a subject that reports less than the floor or fewer than the requested iterations. Every workload/subject measurement runs in a fresh process; the runner re-execs itself for each subscript-jit workload. Only workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject that runs computes the identical integer checksum for a workload — unavailable subjects contribute no checksum, and the runner withholds a workload's timings if any measured checksum differs.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (2.380 ms) | invalid (noise) | 1.61x (3.832 ms) | - | - | invalid (noise) |
| fib-loop | 973132000 | 1.00x (20.711 ms) | 0.99x (20.549 ms) | 2.44x (50.470 ms) | - | - | 1.62x (33.539 ms) |
| mandelbrot | 43027996 | 1.00x (74.937 ms) | 1.02x (76.421 ms) | 0.99x (74.518 ms) | - | - | 0.96x (72.268 ms) |
| primes | 41538 | 1.00x (32.457 ms) | 0.98x (31.745 ms) | 0.99x (32.163 ms) | - | - | 1.00x (32.612 ms) |
| sort | 3672124540 | 1.00x (14.690 ms) | 1.51x (22.166 ms) | 2.01x (29.461 ms) | - | - | 1.59x (23.361 ms) |
| tree | 3932130 | 1.00x (152.830 ms) | 0.65x (99.825 ms) | 3.81x (582.915 ms) | - | - | invalid (noise) |
| queen | 73712 | 1.00x (22.202 ms) | 1.10x (24.503 ms) | 1.54x (34.245 ms) | - | - | 1.35x (30.048 ms) |
| particles | 1712845248 | 1.00x (35.760 ms) | 2.11x (75.324 ms) | 7.05x (252.177 ms) | - | - | 2.89x (103.218 ms) |
| callbacks | -662567840 | 1.00x (13.489 ms) | invalid (noise) | 14.76x (199.097 ms) | - | - | 29.81x (402.047 ms) |
| collect | 1332546592 | 1.00x (38.479 ms) | 1.36x (52.161 ms) | invalid (noise) | - | - | 2.74x (105.351 ms) |

## Measured warm-up

| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|
| fib-recursive | 0.202 s (88 iterations) | 0.201 s (74 iterations) | 0.201 s (51 iterations) | - | - | 0.203 s (29 iterations) |
| fib-loop | 0.211 s (10 iterations) | 0.216 s (10 iterations) | 0.247 s (5 iterations) | - | - | 0.209 s (6 iterations) |
| mandelbrot | 0.228 s (3 iterations) | 0.239 s (3 iterations) | 0.238 s (3 iterations) | - | - | 0.219 s (3 iterations) |
| primes | 0.222 s (7 iterations) | 0.225 s (7 iterations) | 0.200 s (6 iterations) | - | - | 0.215 s (7 iterations) |
| sort | 0.211 s (15 iterations) | 0.205 s (9 iterations) | 0.209 s (7 iterations) | - | - | 0.220 s (9 iterations) |
| tree | 0.457 s (3 iterations) | 0.307 s (3 iterations) | 1.851 s (3 iterations) | - | - | 0.219 s (4 iterations) |
| queen | 0.220 s (10 iterations) | 0.222 s (9 iterations) | 0.211 s (6 iterations) | - | - | 0.209 s (7 iterations) |
| particles | 0.220 s (6 iterations) | 0.225 s (3 iterations) | 0.777 s (3 iterations) | - | - | 0.306 s (3 iterations) |
| callbacks | 0.201 s (15 iterations) | 0.228 s (5 iterations) | 0.615 s (3 iterations) | - | - | 1.347 s (3 iterations) |
| collect | 0.227 s (6 iterations) | 0.250 s (5 iterations) | 0.415 s (3 iterations) | - | - | 0.346 s (3 iterations) |

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

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (42%), fib-recursive/V8 (Node.js) (26%), tree/V8 (Node.js) (40%), callbacks/subscript-ship (23%), collect/subscript-jit (24%) — those timings are invalid and withheld.
