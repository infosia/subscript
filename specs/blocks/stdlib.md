# Standard library — contract

Status: Rev 0, 2026-07-24. P9: `Math` and `Date` v1. Evidence lands in
`specs/tracking/p9-stdlib.md`.

## 0. Design rules (all stdlib, permanent)

1. **The `tsc` side is the ES2022 standard library.** `tsconfig.json`
   already loads `lib: ["ES2022"]`, so `Math` and `Date` are fully
   declared for the editor and the `tsc` gate; the prelude declares
   nothing for them (a redeclaration would collide with the lib). This
   compiler accepts a **deterministic subset** of the lib API with
   sized-type signatures and rejects out-of-subset members with a clear
   S-code — the same shape as rejecting `any`: `tsc` accepts more than
   the language does, never less (invariant 5).
2. **One implementation, both tiers.** Every stdlib operation with a
   runtime component is implemented once, in runtime Rust, and both
   tiers call it through an opaque `sub_rt_*` symbol. The ship tier
   never emits a direct libm call: clang constant-folds recognized libm
   calls at `-O2` with its own evaluator, which is a silent
   dev-JIT ≠ ship-C divergence hazard *(docs)*; an opaque symbol removes
   the fold. The native-builtin pattern (each builtin a Rust function
   over the engine context) is the one used by Rust-implemented JS
   engines — e.g. Boa, <https://github.com/boa-dev/boa> (builtins read
   at the upstream repository; no external engine is an oracle,
   CLAUDE.md).
3. **Determinism.** Every accepted stdlib operation is deterministic
   given the Context state. Nondeterministic inputs (clock, entropy) are
   Context-owned and host-settable, so tests and replays can pin them.
4. **Result semantics are ECMA-262's** for the accepted subset, unless a
   divergence is recorded in `collisions.md` (Q19/Q20). Formatting stays
   Q14 (runtime `fmt_f64`: shortest round-trip, `NaN`, `Infinity`,
   `-0`).

## 1. P9.1 `Math`

Accepted members (all `f64` in and out unless noted; the checker types
them with sized numerics — the lib's `number` view stays assignable
because the sized aliases erase to `number`):

- Unary: `abs, acos, acosh, asin, asinh, atan, atanh, cbrt, ceil, cos,
  cosh, exp, expm1, floor, log, log1p, log10, log2, round, sign, sin,
  sinh, sqrt, tan, tanh, trunc`
- Binary: `atan2(y, x)`, `hypot(a, b)`, `pow(base, exp)`, `max(a, b)`,
  `min(a, b)` — exactly two arguments (the lib's variadic
  `max`/`min`/`hypot` beyond two are rejected, Q19)
- `random(): f64` — §2
- Constants: `E, LN2, LN10, LOG2E, LOG10E, PI, SQRT1_2, SQRT2`, folded
  by the compiler to `f64` literals with the IEEE-754 bit patterns of
  the C `<math.h>`/Rust `f64::consts` doubles (identical bits; the
  shared HIR literal makes the two tiers agree by construction).

Rejected members (S-code + reject-corpus entries): `imul`, `clz32`,
`fround` (JS-number semantics ops; the language has real sized
integers), variadic `max`/`min`/`hypot`.

ECMA edge semantics, pinned by golden: `round` is half-toward-+∞
(`round(-2.5) === -2`); `sign(±0) === ±0`; `max`/`min` propagate `NaN`
and order zeros (`max(+0,-0)=+0`, `min(+0,-0)=-0`); `pow(x, ±0) === 1`
for every `x` including `NaN`; `abs(-0) === +0`.

Lowering: `Math.<fn>(…)` is an ambient-namespace intrinsic call →
`sub_rt_math_<fn>(ctx, args…) -> f64` in both tiers. `Math.<CONST>` is a
member read folded to the literal at check time.

## 2. `Math.random` — Context-seeded, deterministic

`random()` draws from a PRNG owned by the Context: **xoshiro256++**,
seeded by splitmix64 expansion of a `u64` seed; the default seed is
`0x5355_4253_5245_4144` and is part of this contract (the golden pins
the sequence). The draw maps the top 53 bits to `[0, 1)` as
`(x >> 11) as f64 * 2^-53`. A new C API `sub_rt_ctx_seed_random(ctx,
seed: u64)` reseeds (host replay control). Divergence from JS
(unseedable, implementation-entropy) recorded as Q19; determinism is
what a game replay and the golden corpus both need.

## 3. P9.2 `Date` — the UTC-deterministic subset

A `Date` is an **immutable value** that erases to `i64` epoch
milliseconds (proleptic Gregorian, UTC — C `int64_t`). There is no
timezone database, no locale, and no mutation; on this subset the
semantics equal JS on a UTC host.

Accepted API (lib-shaped; checker signatures sized):

- `new Date(ms: i64)`
- `Date.UTC(year: i32, month0: i32, day?: i32, h?: i32, min?: i32,
  s?: i32, ms?: i32): i64` (month is 0-based, as the lib defines)
- `Date.now(): i64` — current UTC millis from the Context clock
- `getTime(): i64`
- `getUTCFullYear/Month/Date/Day/Hours/Minutes/Seconds/Milliseconds():
  i32`
- `toISOString(): string` — `YYYY-MM-DDTHH:mm:ss.sssZ`, zero-padded,
  years 0000–9999

Rejected (S-code + reject entries): every local-time accessor
(`getFullYear`, `getMonth`, …), every setter (`setTime`, …), `parse`,
`toLocaleString`/`toString` family, the multi-argument constructor
`new Date(y, m, …)` (the lib interprets it in *local* time — accepting
it with UTC semantics would silently change meaning; write
`new Date(Date.UTC(y, m, …))`), and template interpolation of a `Date`
(write `toISOString()`).

Range and errors: valid times are `|ms| ≤ 8.64e15` (the ECMA TimeClip
range) and `toISOString` additionally requires years 0000–9999. Out of
range **traps** with a clear report — there is no Invalid-Date value
(divergence from JS NaN-dates, Q20; invariant 6: early errors).

Clock: the Context holds the `Date.now` source — default is the system
UTC clock; `sub_rt_ctx_set_now(ctx, ms: i64)` pins it (tests, replays).
Corpus entries do not call `Date.now()` (nondeterministic under the
default); `now()` is covered by runtime unit tests and a both-tier test
with a pinned clock.

Calendar algorithms (civil↔days conversion, day-of-week) are implemented
in runtime Rust with direct unit tests: epoch, leap rules (2000-02-29
valid, 1900 and 2100 not leap, 400-year rule), pre-1970 negatives, and
known weekdays. Lowering: constructor/statics/methods are intrinsics →
`sub_rt_date_*` on both tiers.

## 4. Corpus plan (Red first)

Accept: `a40` Math battery (functions, constants, the §1 edge pins,
`NaN`/`-0` formatting); `a41` random sequence (default seed, first
draws); `a42` Date battery (construction via `Date.UTC`, accessors,
`toISOString` round-trips, leap dates, pre-1970). Reject: `imul`,
three-argument `max`, `getFullYear`, `setTime`, multi-argument `Date`
constructor, `Date` in a template literal — each with its S-code.
Every accept entry stays `tsc`-clean (lib-typed) — the standing gate.

## 5. Gate (pre-registered exit criteria)

1. Standing differential gate byte-exact on every entry including
   a40–a42, both tiers.
2. `tsc -p tsconfig.json` zero errors, unchanged config.
3. The §1 ECMA edge battery and the §2 PRNG sequence are pinned in
   committed goldens; the date unit tests of §3 pass.
4. Reject entries produce their named S-codes.
5. Benchmarks: no ship-row regression (stdlib adds no cost to programs
   that do not use it).

## 6. Staging

P9.1 `Math` (ambient-namespace checker machinery + runtime + a40/a41 +
rejects); P9.2 `Date` (ambient nominal value type + calendar runtime +
a42 + rejects). Phase Review per stage.
