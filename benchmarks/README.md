# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ d19e304 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.816 ms) | 0.98x (4.697 ms) | 1.67x (8.035 ms) | 1.52x (7.305 ms) | 1.13x (5.420 ms) | 1.98x (9.550 ms) |
| fib-loop | 973132000 | 1.00x (29.243 ms) | 1.03x (30.122 ms) | 2.08x (60.772 ms) | 1.52x (44.473 ms) | 1.09x (31.960 ms) | 1.63x (47.599 ms) |
| mandelbrot | 43027996 | 1.00x (124.020 ms) | 1.00x (124.116 ms) | 1.05x (130.134 ms) | 2.84x (352.725 ms) | 1.00x (124.480 ms) | 1.01x (125.060 ms) |
| primes | 41538 | 1.00x (21.799 ms) | 0.97x (21.243 ms) | 1.45x (31.635 ms) | 2.09x (45.547 ms) | 0.92x (20.080 ms) | 1.71x (37.258 ms) |
| sort | 3672124540 | 1.00x (15.208 ms) | 1.81x (27.572 ms) | 3.73x (56.698 ms) | 2.29x (34.873 ms) | 1.50x (22.840 ms) | 1.84x (28.003 ms) |
| tree | 3932130 | 1.00x (65.257 ms) | 1.36x (88.856 ms) | 10.23x (667.377 ms) | 2.18x (142.420 ms) | 0.32x (21.160 ms) | 0.47x (30.508 ms) |
| queen | 73712 | 1.00x (23.719 ms) | 0.99x (23.571 ms) | 1.47x (34.910 ms) | 1.50x (35.522 ms) | 1.22x (28.920 ms) | 1.75x (41.462 ms) |
| particles | 1712845248 | 1.00x (38.677 ms) | 3.06x (118.487 ms) | 10.38x (401.342 ms) | 3.83x (148.304 ms) | 1.90x (73.640 ms) | 3.58x (138.484 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (38%) — treat those rows as indicative.
