# Cross-language benchmarks — captured results

Snapshot captured 2026-07-25. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 8bcbbec (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.431 ms) | 1.07x (4.734 ms) | 1.87x (8.303 ms) | 1.55x (6.870 ms) | 1.23x (5.440 ms) | 2.17x (9.630 ms) |
| fib-loop | 973132000 | 1.00x (29.522 ms) | 1.03x (30.348 ms) | 2.07x (61.052 ms) | 1.51x (44.706 ms) | 1.09x (32.160 ms) | 1.59x (46.956 ms) |
| mandelbrot | 43027996 | 1.00x (125.159 ms) | 1.00x (125.020 ms) | 1.05x (131.083 ms) | 2.82x (352.992 ms) | 1.00x (125.360 ms) | 1.01x (126.185 ms) |
| primes | 41538 | 1.00x (22.047 ms) | 0.97x (21.495 ms) | 1.51x (33.318 ms) | 2.09x (46.001 ms) | 0.92x (20.200 ms) | 1.70x (37.514 ms) |
| sort | 3672124540 | 1.00x (15.473 ms) | 1.80x (27.795 ms) | 3.69x (57.081 ms) | 2.27x (35.170 ms) | 1.48x (22.920 ms) | 1.82x (28.225 ms) |
| tree | 3932130 | 1.00x (65.680 ms) | 1.37x (89.738 ms) | 10.50x (689.533 ms) | 2.23x (146.502 ms) | 0.33x (21.560 ms) | 0.47x (31.191 ms) |
| queen | 73712 | 1.00x (23.893 ms) | 1.00x (23.814 ms) | 1.48x (35.418 ms) | 1.59x (37.952 ms) | 1.23x (29.360 ms) | 1.76x (42.006 ms) |
| particles | 1712845248 | 1.00x (39.119 ms) | 3.06x (119.636 ms) | 10.28x (402.330 ms) | 3.82x (149.603 ms) | 1.90x (74.300 ms) | 3.58x (139.946 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (43%) — treat those rows as indicative.
