# Cross-language benchmarks — captured results

Snapshot captured 2026-07-25. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: Apple M2 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 4164fe3 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.809 ms) | 0.97x (4.656 ms) | 1.68x (8.059 ms) | 1.39x (6.707 ms) | 1.13x (5.440 ms) | 2.00x (9.598 ms) |
| fib-loop | 973132000 | 1.00x (29.325 ms) | 1.03x (30.241 ms) | 2.09x (61.164 ms) | 1.52x (44.695 ms) | 1.12x (32.900 ms) | 1.63x (47.882 ms) |
| mandelbrot | 43027996 | 1.00x (125.365 ms) | 1.00x (124.951 ms) | 1.06x (133.161 ms) | 2.84x (356.196 ms) | 1.00x (125.520 ms) | 1.01x (126.373 ms) |
| primes | 41538 | 1.00x (22.037 ms) | 0.97x (21.345 ms) | 1.44x (31.795 ms) | 2.13x (46.904 ms) | 0.94x (20.680 ms) | 1.74x (38.381 ms) |
| sort | 3672124540 | 1.00x (15.691 ms) | 1.77x (27.839 ms) | 3.64x (57.062 ms) | 2.24x (35.093 ms) | 1.47x (23.020 ms) | 1.79x (28.137 ms) |
| tree | 3932130 | 1.00x (65.578 ms) | 1.39x (91.217 ms) | 10.45x (685.010 ms) | 2.25x (147.395 ms) | 0.32x (21.020 ms) | 0.47x (30.797 ms) |
| queen | 73712 | 1.00x (23.772 ms) | 1.00x (23.678 ms) | 1.48x (35.128 ms) | 1.49x (35.427 ms) | 1.22x (29.080 ms) | 1.76x (41.928 ms) |
| particles | 1712845248 | 1.00x (38.872 ms) | 3.08x (119.624 ms) | 10.32x (400.984 ms) | 3.83x (148.991 ms) | 1.90x (73.920 ms) | 3.58x (139.354 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (36%) — treat those rows as indicative.
