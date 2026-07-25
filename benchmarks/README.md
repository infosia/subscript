# Cross-language benchmarks — captured results

Snapshot captured 2026-07-25. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 4529068 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 3 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.677 ms) | 0.98x (4.599 ms) | 1.72x (8.035 ms) | 1.45x (6.791 ms) | 1.16x (5.420 ms) | 2.06x (9.614 ms) |
| fib-loop | 973132000 | 1.00x (29.336 ms) | 1.03x (30.138 ms) | 2.02x (59.149 ms) | 1.52x (44.456 ms) | 1.09x (32.060 ms) | 1.58x (46.312 ms) |
| mandelbrot | 43027996 | 1.00x (124.295 ms) | 1.00x (124.335 ms) | 1.05x (130.337 ms) | 2.82x (349.971 ms) | 1.00x (124.580 ms) | 1.01x (125.234 ms) |
| primes | 41538 | 1.00x (21.874 ms) | 0.97x (21.274 ms) | 1.45x (31.629 ms) | 2.08x (45.591 ms) | 0.92x (20.120 ms) | 1.70x (37.151 ms) |
| sort | 3672124540 | 1.00x (15.191 ms) | 1.82x (27.635 ms) | 3.74x (56.751 ms) | 2.30x (34.872 ms) | 1.51x (22.880 ms) | 1.84x (27.981 ms) |
| tree | 3932130 | 1.00x (65.226 ms) | 1.39x (90.612 ms) | 10.28x (670.371 ms) | 2.26x (147.153 ms) | 0.32x (20.700 ms) | 0.46x (30.200 ms) |
| queen | 73712 | 1.00x (23.731 ms) | 0.99x (23.611 ms) | 1.48x (35.072 ms) | 1.49x (35.367 ms) | 1.22x (28.980 ms) | 1.75x (41.620 ms) |
| particles | 1712845248 | 1.00x (38.676 ms) | 3.07x (118.611 ms) | 10.30x (398.351 ms) | 3.83x (148.293 ms) | 1.91x (73.700 ms) | 3.58x (138.600 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (29%) — treat those rows as indicative.
