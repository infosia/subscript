# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 821170e (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.758 ms) | 0.99x (4.703 ms) | 1.67x (7.934 ms) | 1.49x (7.088 ms) | 1.14x (5.420 ms) | 2.02x (9.607 ms) |
| fib-loop | 973132000 | 1.00x (29.678 ms) | 1.02x (30.412 ms) | 2.01x (59.663 ms) | 1.50x (44.408 ms) | 1.09x (32.440 ms) | 1.58x (46.922 ms) |
| mandelbrot | 43027996 | 1.00x (125.305 ms) | 1.00x (125.747 ms) | 1.05x (131.549 ms) | 2.78x (348.857 ms) | 1.00x (125.580 ms) | 1.01x (126.856 ms) |
| primes | 41538 | 1.00x (22.257 ms) | 0.96x (21.372 ms) | 1.44x (32.086 ms) | 2.06x (45.941 ms) | 0.92x (20.440 ms) | 1.69x (37.603 ms) |
| sort | 3672124540 | 1.00x (15.445 ms) | 1.77x (27.342 ms) | 3.70x (57.109 ms) | 2.28x (35.185 ms) | 1.45x (22.340 ms) | 1.83x (28.194 ms) |
| tree | 3932130 | 1.00x (65.707 ms) | 1.37x (89.824 ms) | 10.42x (684.481 ms) | 2.20x (144.628 ms) | 0.33x (21.580 ms) | 0.47x (30.870 ms) |
| queen | 73712 | 1.00x (23.806 ms) | 0.99x (23.654 ms) | 1.48x (35.342 ms) | 1.54x (36.704 ms) | 1.23x (29.220 ms) | 1.76x (42.004 ms) |
| particles | 1712845248 | 1.00x (38.978 ms) | 3.07x (119.510 ms) | 10.35x (403.269 ms) | 3.84x (149.515 ms) | 1.90x (74.240 ms) | 3.58x (139.565 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (41%) — treat those rows as indicative.
