# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 50326f7 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.665 ms) | 0.97x (4.548 ms) | 1.76x (8.229 ms) | 1.48x (6.889 ms) | 1.17x (5.460 ms) | 2.13x (9.921 ms) |
| fib-loop | 973132000 | 1.00x (30.105 ms) | 1.02x (30.690 ms) | 2.00x (60.219 ms) | 1.49x (44.799 ms) | 1.09x (32.820 ms) | 1.57x (47.364 ms) |
| mandelbrot | 43027996 | 1.00x (126.902 ms) | 1.00x (126.464 ms) | 1.04x (132.220 ms) | 2.79x (354.211 ms) | 1.00x (127.020 ms) | 1.00x (127.194 ms) |
| primes | 41538 | 1.00x (22.424 ms) | 0.96x (21.515 ms) | 1.44x (32.283 ms) | 2.11x (47.424 ms) | 0.91x (20.480 ms) | 1.69x (37.794 ms) |
| sort | 3672124540 | 1.00x (15.457 ms) | 1.79x (27.722 ms) | 3.73x (57.693 ms) | 2.31x (35.654 ms) | 1.49x (23.060 ms) | 1.84x (28.404 ms) |
| tree | 3932130 | 1.00x (66.290 ms) | 5.11x (338.930 ms) | 10.61x (703.202 ms) | 2.25x (149.086 ms) | 0.33x (21.880 ms) | 0.47x (31.463 ms) |
| queen | 73712 | 1.00x (24.225 ms) | 0.99x (24.061 ms) | 1.46x (35.457 ms) | 1.52x (36.928 ms) | 1.21x (29.340 ms) | 1.75x (42.279 ms) |
| particles | 1712845248 | 1.00x (39.351 ms) | 3.06x (120.259 ms) | 10.30x (405.207 ms) | 3.83x (150.582 ms) | 1.91x (75.120 ms) | 3.57x (140.502 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (32%) — treat those rows as indicative.
