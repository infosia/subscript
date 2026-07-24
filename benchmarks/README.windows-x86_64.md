# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: x86_64 / windows
- CPU: Intel64 Family 6 Model 198 Stepping 2, GenuineIntel (20 logical cores)
- power: unknown

## Runtimes

- **C**: clang version 22.1.6 (https://github.com/llvm/llvm-project fc4aad7b5db3fff421df9a9637605b9ca5667881)
- **subscript**: subscript @ 3dc3695 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: absent
- **JSC**: absent
- **V8 (Node.js)**: Node.js v24.16.0

## Method

All six subjects run the same schedule: 20 warm-up runs discarded, 21 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (2.363 ms) | 1.03x (2.442 ms) | 1.54x (3.648 ms) | - | - | 2.88x (6.800 ms) |
| fib-loop | 973132000 | 1.00x (21.758 ms) | 1.00x (21.866 ms) | 2.31x (50.347 ms) | - | - | 1.69x (36.867 ms) |
| mandelbrot | 43027996 | 1.00x (79.208 ms) | 1.01x (79.819 ms) | 1.01x (80.191 ms) | - | - | 0.99x (78.566 ms) |
| primes | 41538 | 1.00x (34.293 ms) | 0.99x (33.786 ms) | 0.97x (33.419 ms) | - | - | 0.97x (33.411 ms) |
| sort | 3672124540 | 1.00x (14.434 ms) | 1.84x (26.551 ms) | 3.46x (49.966 ms) | - | - | 1.72x (24.814 ms) |
| tree | 3932130 | 1.00x (117.662 ms) | 0.81x (95.081 ms) | 12.49x (1470.043 ms) | - | - | 0.60x (70.510 ms) |
| queen | 73712 | 1.00x (22.902 ms) | 0.99x (22.671 ms) | 1.67x (38.223 ms) | - | - | 1.34x (30.683 ms) |
| particles | 1712845248 | 1.00x (39.806 ms) | 2.36x (93.980 ms) | 8.07x (321.079 ms) | - | - | 2.84x (112.951 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (43%), fib-recursive/subscript-jit (27%), tree/subscript-ship (63%) — treat those rows as indicative.
