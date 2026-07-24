# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 58c0a1a (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.886 ms) | 0.93x (4.565 ms) | 1.65x (8.060 ms) | 1.41x (6.912 ms) | 1.11x (5.440 ms) | 1.96x (9.594 ms) |
| fib-loop | 973132000 | 1.00x (29.333 ms) | 1.05x (30.700 ms) | 2.03x (59.508 ms) | 1.48x (43.468 ms) | 1.10x (32.140 ms) | 1.60x (46.841 ms) |
| mandelbrot | 43027996 | 1.00x (124.952 ms) | 1.00x (125.270 ms) | 1.05x (131.109 ms) | 2.80x (349.712 ms) | 1.03x (128.460 ms) | 1.08x (135.463 ms) |
| primes | 41538 | 1.00x (22.590 ms) | 0.98x (22.178 ms) | 1.44x (32.592 ms) | 2.04x (46.125 ms) | 0.89x (20.160 ms) | 1.71x (38.566 ms) |
| sort | 3672124540 | 1.00x (15.650 ms) | 1.77x (27.695 ms) | 3.69x (57.751 ms) | 2.26x (35.361 ms) | 1.48x (23.160 ms) | 1.81x (28.256 ms) |
| tree | 3932130 | 1.00x (66.277 ms) | 1.37x (90.707 ms) | 10.55x (699.290 ms) | 2.24x (148.393 ms) | 0.33x (21.720 ms) | 0.47x (31.075 ms) |
| queen | 73712 | 1.00x (23.930 ms) | 0.99x (23.792 ms) | 1.49x (35.654 ms) | 1.50x (35.835 ms) | 1.23x (29.460 ms) | 1.76x (42.145 ms) |
| particles | 1712845248 | 1.00x (39.221 ms) | 3.07x (120.287 ms) | 10.32x (404.886 ms) | 3.85x (150.867 ms) | 1.93x (75.740 ms) | 3.60x (141.024 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (28%) — treat those rows as indicative.
