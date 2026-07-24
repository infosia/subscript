# P9 stdlib — evidence

Contract: `specs/blocks/stdlib.md`; collisions Q19/Q20; compiler block
§15.

## P9.1 — `Math`: COMPLETE (2026-07-24)

Checker: `Math.<fn>(…)` resolves as an ambient-namespace intrinsic
(`Callee::Math`, 32 variants), f64-typed with exact arity; the 8
constants fold at check time to f64 literals bit-identical to the C
`<math.h>` doubles. S014 rejects out-of-subset members (`imul`,
`clz32`, `fround`), wrong arity (incl. 3-arg `max`/`min`/`hypot`),
member writes, un-called member reads, and `Math` as a value (Q19).
User declarations named `Math` shadow the namespace in every form
(class, const, parameter — probe-verified).

Runtime: `runtime/src/math.rs`, one implementation; both tiers call
opaque `sub_rt_math_*` (the ship C emitter never emits a bare libm
call — clang's libm constant folding is the pre-registered §0.2
hazard; a constant-argument fold probe is byte-identical across
tiers). ECMA edges implemented and probe-verified beyond the pinned
corpus: `round` half-toward-+∞ incl. `round(0.49999999999999994)=+0`
(the naive `floor(x+0.5)` misround is absent) and the 2^52 boundary;
`sign`/`trunc`/`ceil` signed zeros; `sqrt(-0)=-0`; `expm1`/`log1p`
`-0`; `atanh(±1)=±Inf`; `max`/`min` NaN propagation + zero ordering;
`pow` ECMA table.

`Math.random`: Context-owned xoshiro256++ seeded by splitmix64 from
the contract seed `0x5355_4253_5245_4144`; draw maps the top 53 bits
to [0,1). An independent reimplementation from the published algorithm
matched 64 draws bit-exactly; the unit-test pin, the a41 golden, and
that reimplementation agree. `sub_rt_ctx_seed_random` reseeds; dev and
ship Context constructions share the stream.

Corpus: a40 (every §1 function, all constants, the edge battery), a41
(pinned 8-draw sequence); rejects r15–r18 (S014). Golden floor 41.

Phase Review (2026-07-24, fresh no-context): 0 CRITICAL, 2 MAJOR,
2 MINOR.

- MAJOR 1 (fixed, `b248844`): `Math.pow(1, NaN)` returned 1 — ECMA
  `Number::exponentiate` step 1 returns NaN for a NaN exponent with no
  base-1 exception; IEEE `powf(1, NaN)=1` diverged and the guard only
  covered infinite exponents. Fixed with a NaN-exponent guard;
  `pow(x, ±0)=1` (all x, incl. NaN base) preserved; pinned by unit
  test + the a40 line `pow(1,NaN) NaN`.
- MAJOR 2 (this file): the tracking entry and the §5.5 benchmark row
  were unrecorded. §5.5 evidence below.
- MINOR 1 (recorded): `Math.PI(1.0)` is rejected soundly but as S100
  ("f64 is not callable" — the constant folds first), not S014 naming
  the member. Wording only.
- MINOR 2 (fixed, `b248844`): the emitted-C libm negative assertion
  was space-prefix substring matching; now token-boundary
  (`has_bare_call`).

§5 gate evidence: standing gate byte-exact on all 41 entries, both
tiers (incl. cranelift-object cross-check); `tsc` zero errors,
config unchanged; edge battery + PRNG sequence pinned in committed
goldens; r15–r18 assert S014 at pinned positions; §5.5 benchmark run
at `b248844` — ship rows unchanged (tree 1.36×, sort 1.76×,
particles 3.06×, compute-bound 0.97–1.00×; run noise only).
`cargo test --offline` 304/0, zero warnings.

Next: P9.2 — `Date` (stdlib.md §3).
