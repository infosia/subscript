# Cross-language benchmarks — captured results

Snapshot captured 2026-07-25. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 568293b (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.713 ms) | 1.02x (4.801 ms) | 1.71x (8.056 ms) | 1.54x (7.272 ms) | 1.16x (5.460 ms) | 2.03x (9.586 ms) |
| fib-loop | 973132000 | 1.00x (30.110 ms) | 1.01x (30.379 ms) | 2.01x (60.500 ms) | 1.48x (44.672 ms) | 1.07x (32.220 ms) | 1.59x (47.805 ms) |
| mandelbrot | 43027996 | 1.00x (124.875 ms) | 1.00x (125.009 ms) | 1.05x (131.101 ms) | 2.83x (353.946 ms) | 1.00x (125.400 ms) | 1.01x (125.976 ms) |
| primes | 41538 | 1.00x (22.309 ms) | 0.96x (21.339 ms) | 1.42x (31.710 ms) | 2.11x (46.986 ms) | 0.90x (20.140 ms) | 1.72x (38.311 ms) |
| sort | 3672124540 | 1.00x (15.646 ms) | 1.77x (27.731 ms) | 3.61x (56.553 ms) | 2.25x (35.150 ms) | 1.46x (22.880 ms) | 1.80x (28.130 ms) |
| tree | 3932130 | 1.00x (65.447 ms) | 1.37x (89.338 ms) | 10.40x (680.660 ms) | 2.24x (146.357 ms) | 0.33x (21.360 ms) | 0.48x (31.209 ms) |
| queen | 73712 | 1.00x (23.806 ms) | 1.00x (23.698 ms) | 1.48x (35.326 ms) | 1.50x (35.698 ms) | 1.22x (29.080 ms) | 1.77x (42.044 ms) |
| particles | 1712845248 | 1.00x (38.961 ms) | 3.06x (119.150 ms) | 10.29x (401.089 ms) | 3.83x (149.135 ms) | 1.90x (73.900 ms) | 3.60x (140.148 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (46%) — treat those rows as indicative.
