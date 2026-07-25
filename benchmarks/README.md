# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ d42133e (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.153 ms) | 1.15x (4.756 ms) | 1.92x (7.956 ms) | 1.71x (7.084 ms) | 1.31x (5.420 ms) | 2.34x (9.697 ms) |
| fib-loop | 973132000 | 1.00x (29.902 ms) | 1.02x (30.555 ms) | 2.00x (59.764 ms) | 1.47x (43.849 ms) | 1.08x (32.400 ms) | 1.57x (46.830 ms) |
| mandelbrot | 43027996 | 1.00x (125.721 ms) | 1.00x (125.696 ms) | 1.05x (131.698 ms) | 2.78x (349.534 ms) | 1.00x (125.800 ms) | 1.01x (126.464 ms) |
| primes | 41538 | 1.00x (22.065 ms) | 0.97x (21.447 ms) | 1.45x (31.940 ms) | 2.09x (46.086 ms) | 0.92x (20.320 ms) | 1.70x (37.549 ms) |
| sort | 3672124540 | 1.00x (16.186 ms) | 1.69x (27.355 ms) | 3.54x (57.239 ms) | 2.18x (35.269 ms) | 1.42x (22.960 ms) | 1.75x (28.283 ms) |
| tree | 3932130 | 1.00x (66.013 ms) | 1.36x (89.869 ms) | 10.40x (686.345 ms) | 2.25x (148.537 ms) | 0.32x (21.320 ms) | 0.48x (31.569 ms) |
| queen | 73712 | 1.00x (23.999 ms) | 0.98x (23.579 ms) | 1.48x (35.477 ms) | 1.45x (34.882 ms) | 1.22x (29.380 ms) | 1.75x (41.934 ms) |
| particles | 1712845248 | 1.00x (38.987 ms) | 3.07x (119.819 ms) | 10.43x (406.735 ms) | 3.84x (149.824 ms) | 1.91x (74.480 ms) | 3.59x (139.909 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (45%) — treat those rows as indicative.
