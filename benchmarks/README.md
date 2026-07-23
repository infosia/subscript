# Cross-language benchmarks — captured results

Snapshot captured 2026-07-23. Measured live by the runner (`benchmarks/runner.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-bench --bin benchmarks`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: Apple M2 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 4ba01f9 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 20 warm-up runs discarded, 21 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.638 ms) | 1.00x (3.636 ms) | 2.15x (7.832 ms) | 1.84x (6.708 ms) | 1.49x (5.420 ms) | 2.64x (9.619 ms) |
| fib-loop | 973132000 | 1.00x (29.393 ms) | 1.03x (30.162 ms) | 1.99x (58.469 ms) | 1.48x (43.392 ms) | 1.09x (32.080 ms) | 1.58x (46.401 ms) |
| mandelbrot | 43027996 | 1.00x (124.325 ms) | 1.00x (124.399 ms) | 1.05x (131.097 ms) | 2.78x (345.399 ms) | 1.00x (124.620 ms) | 1.01x (125.335 ms) |
| primes | 41538 | 1.00x (21.940 ms) | 0.97x (21.320 ms) | 1.44x (31.678 ms) | 2.08x (45.619 ms) | 0.92x (20.140 ms) | 1.69x (37.187 ms) |
| sort | 3672124540 | 1.00x (15.247 ms) | 1.78x (27.133 ms) | 3.73x (56.887 ms) | 2.30x (35.048 ms) | 1.45x (22.180 ms) | 1.84x (28.001 ms) |
| tree | 3932130 | 1.00x (65.349 ms) | 10.07x (657.756 ms) | 10.28x (671.581 ms) | 2.19x (142.825 ms) | 0.30x (19.860 ms) | 0.47x (30.978 ms) |
| queen | 73712 | 1.00x (23.720 ms) | 1.00x (23.645 ms) | 1.47x (34.952 ms) | 1.51x (35.921 ms) | 1.22x (29.020 ms) | 1.76x (41.679 ms) |
| particles | 1712845248 | 1.00x (38.742 ms) | 3.06x (118.677 ms) | 10.29x (398.566 ms) | 3.84x (148.733 ms) | 1.90x (73.780 ms) | 3.58x (138.714 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: every subscript-tier sample set is within +/-20% of its median.
