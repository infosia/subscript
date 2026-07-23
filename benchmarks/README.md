# Cross-language benchmarks — captured results

Snapshot captured 2026-07-23. Measured live by the runner (`benchmarks/runner.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-bench --bin benchmarks`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: unknown (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ ba2c568 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

5 warm-up runs discarded, 11 timed runs, median reported. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (4.754 ms) | 0.93x (4.414 ms) | 1.66x (7.905 ms) | 1.43x (6.796 ms) | 1.14x (5.420 ms) | 2.04x (9.675 ms) |
| fib-loop | 973132000 | 1.00x (29.553 ms) | 1.03x (30.362 ms) | 2.01x (59.492 ms) | 1.48x (43.708 ms) | 1.09x (32.120 ms) | 1.58x (46.714 ms) |
| mandelbrot | 43027996 | 1.00x (125.631 ms) | 1.00x (125.502 ms) | 1.05x (131.567 ms) | 2.77x (348.214 ms) | 1.00x (125.540 ms) | 1.01x (126.525 ms) |
| primes | 41538 | 1.00x (22.024 ms) | 0.98x (21.525 ms) | 1.45x (32.032 ms) | 2.10x (46.194 ms) | 0.92x (20.320 ms) | 1.70x (37.437 ms) |
| sort | 3672124540 | 1.00x (15.272 ms) | 1.78x (27.210 ms) | 3.75x (57.319 ms) | 2.30x (35.125 ms) | 1.50x (22.880 ms) | 1.85x (28.207 ms) |
| tree | 3932130 | 1.00x (65.675 ms) | 10.23x (672.154 ms) | 10.49x (688.879 ms) | 2.20x (144.572 ms) | 0.32x (21.100 ms) | 0.47x (31.114 ms) |
| queen | 73712 | 1.00x (23.681 ms) | 1.00x (23.732 ms) | 1.49x (35.181 ms) | 1.49x (35.215 ms) | 1.23x (29.200 ms) | 2.55x (60.399 ms) |
| particles | 1712845248 | 1.00x (39.344 ms) | 3.04x (119.605 ms) | 10.23x (402.650 ms) | 3.80x (149.544 ms) | 1.89x (74.300 ms) | 3.55x (139.826 ms) |

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32

Noise: wider than +/-20% spread for fib-recursive/subscript-ship (24%) — treat those rows as indicative.
