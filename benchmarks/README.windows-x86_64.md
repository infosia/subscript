# Cross-language benchmarks — captured results

Snapshot captured 2026-07-24. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: x86_64 / windows
- CPU: Intel64 Family 6 Model 198 Stepping 2, GenuineIntel (20 logical cores)
- power: unknown

## Runtimes

- **C**: clang version 22.1.6 (https://github.com/llvm/llvm-project fc4aad7b5db3fff421df9a9637605b9ca5667881)
- **subscript**: subscript @ 69c3739 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: absent
- **JSC**: absent
- **V8 (Node.js)**: Node.js v24.16.0

## Method

All six subjects run the same schedule: 20 warm-up runs discarded, 21 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (2.387 ms) | 1.00x (2.382 ms) | 1.54x (3.672 ms) | - | - | 2.86x (6.824 ms) |
| fib-loop | 973132000 | 1.00x (21.719 ms) | 0.99x (21.487 ms) | 2.24x (48.627 ms) | - | - | 1.69x (36.668 ms) |
| mandelbrot | 43027996 | 1.00x (78.509 ms) | 1.00x (78.679 ms) | 1.00x (78.704 ms) | - | - | 0.99x (77.680 ms) |
| primes | 41538 | 1.00x (33.026 ms) | 1.00x (33.147 ms) | 1.02x (33.700 ms) | - | - | 1.03x (33.962 ms) |
| sort | 3672124540 | 1.00x (14.062 ms) | 1.91x (26.796 ms) | 3.11x (43.753 ms) | - | - | 1.73x (24.281 ms) |
| tree | 3932130 | 1.00x (103.142 ms) | 5.33x (549.399 ms) | 11.09x (1144.333 ms) | - | - | 0.61x (62.922 ms) |
| queen | 73712 | 1.00x (22.956 ms) | 0.95x (21.856 ms) | 1.65x (37.833 ms) | - | - | 1.35x (30.886 ms) |
| particles | 1712845248 | 1.00x (39.755 ms) | 2.24x (89.121 ms) | 7.92x (314.892 ms) | - | - | 2.69x (107.025 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.

Noise: wider than +/-20% spread for fib-recursive/subscript-jit (66%), sort/subscript-ship (22%) — treat those rows as indicative.
