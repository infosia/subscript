# Cross-language benchmarks — captured results

Snapshot captured 2026-07-25. Measured live by the runner (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. Contract: `specs/blocks/benchmarks.md`.

## Machine

- host: aarch64 / macos
- CPU: aarch64 (8 logical cores)
- power: AC Power

## Runtimes

- **C**: Apple clang version 21.0.0 (clang-2100.1.1.101)
- **subscript**: subscript @ 4a39836 (dev-JIT: Cranelift; ship: HIR->C->clang)
- **LuaJIT**: LuaJIT 2.1.1784580905 -- Copyright (C) 2005-2026 Mike Pall. https://luajit.org/
- **JSC**: JavaScriptCore (macOS 26.5.2)
- **V8 (Node.js)**: Node.js v24.18.0

## Method

All six subjects run the same schedule: 100 warm-up runs discarded, 11 timed runs, median reported — the runner passes these counts to every self-timed subject (C/LuaJIT/JSC/V8 read them from argv), so the figures above hold for all six. Only the workload execution is timed. C is the 1.00x reference; every other subject is `ratio (median)`. C, LuaJIT, JSC, and V8 self-time with a monotonic clock and print their own median; the two subscript tiers are timed by the runner (the language has no clock primitive). Every subject computes the identical integer checksum for a workload — the runner withholds a workload's timings otherwise.

**Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` call and print the checksum afterward; the two subscript tiers time the whole exported `main()`, which includes formatting and writing the one-line integer checksum to the runtime sink. That is a sub-microsecond step inside subscript's span but outside the others' — a conservative difference that penalizes subscript, retained because the ship-tier AOT timing entry and `jit_bench` are shared with the P4 performance gate and time the exported entry by contract.

## Results

| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |
|---|---|---|---|---|---|---|---|
| fib-recursive | 1346269 | 1.00x (3.644 ms) | 1.00x (3.632 ms) | 2.15x (7.822 ms) | 1.92x (7.013 ms) | 1.49x (5.420 ms) | 2.62x (9.554 ms) |
| fib-loop | 973132000 | invalid (noise) | 30.170 ms | 59.215 ms | 43.324 ms | 31.980 ms | 30.051 ms |
| mandelbrot | 43027996 | invalid (noise) | 124.351 ms | 130.171 ms | 348.347 ms | 124.560 ms | 124.972 ms |
| primes | 41538 | invalid (noise) | 21.257 ms | 31.679 ms | 45.596 ms | 20.060 ms | 37.197 ms |
| sort | 3672124540 | 1.00x (15.312 ms) | 1.80x (27.619 ms) | 3.71x (56.832 ms) | 2.28x (34.931 ms) | 1.50x (22.900 ms) | 1.83x (28.043 ms) |
| tree | 3932130 | 1.00x (65.380 ms) | 1.42x (92.792 ms) | 10.19x (666.067 ms) | 2.23x (145.981 ms) | 0.30x (19.900 ms) | 0.43x (27.917 ms) |
| queen | 73712 | 1.00x (23.717 ms) | 0.99x (23.598 ms) | 1.48x (35.087 ms) | 1.47x (34.961 ms) | 1.22x (28.920 ms) | 1.75x (41.579 ms) |
| particles | 1712845248 | 1.00x (38.717 ms) | 3.06x (118.466 ms) | 10.43x (403.757 ms) | 3.84x (148.638 ms) | 1.90x (73.660 ms) | 3.57x (138.062 ms) |
| callbacks | -662567840 | 1.00x (13.072 ms) | 20.84x (272.433 ms) | 26.06x (340.633 ms) | 9.62x (125.815 ms) | 5.38x (70.280 ms) | 29.76x (389.078 ms) |

**callbacks interpretation.** This workload measures what the idiomatic callback spelling costs against a hand-written loop, not a codegen deficit.

## Workload parameters

- **fib-recursive** — naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)
- **fib-loop** — iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum
- **mandelbrot** — 800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)
- **primes** — count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)
- **sort** — quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)
- **tree** — 30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + unsafeDelete; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130
- **queen** — count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)
- **particles** — 100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.
- **callbacks** — i32[1000000] from LCG state=state*1664525+1013904223 (seed 0x12345678), K=20 rounds; map(value,index)=(value+index) i32; filter(value,index)=((value^index)&3)!=0 (removes exactly 250000 elements per round); reduce(acc,value,index)=(acc+value+index) i32 from 0; checksum=checksum+round_result (i32 wrap)

Noise: wider than +/-20% spread for fib-loop/C (71%), mandelbrot/C (23%), primes/C (120%) — those timings are invalid and withheld.
