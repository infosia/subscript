# Standard library — contract

Status: Rev 1, 2026-07-25 (Rev 0: 2026-07-24, P9 `Math`/`Date`; Rev 1 adds the §7 stdlib roadmap and the §8 P10 `String` contract; Rev 2, 2026-07-25, adds the §9 P11 `Array` contract; Rev 3, 2026-07-25, reverses the `Map`/`Set` non-goal and cross-references P14 narrow numerics). Evidence lands in
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

## 7. Roadmap — the rest of the standard library (Rev 1)

Ordered by value to game scripts and by machinery dependency; each
phase follows the standing pattern (§0) and the workflow loop
(contract → corpus Red → implement → gate → review). A phase's
detailed contract lands in this file before its implementation opens.

| Phase | Area | New machinery | Status |
|---|---|---|---|
| P10 | `String` methods (§8) | none (extends the Str member surface) | contract below |
| P11 | `Array` methods (§9) | runtime→script comparator/predicate calls (non-escaping closures, C5) | contract below |
| P12 | `Number` statics + `parseInt`/`parseFloat`/`toFixed` | none | contract before open |
| P13 | `JSON` | typed serialization over layout descriptors (RTTI) — needs its own design | contract before open |
| P15 | `Map`/`Set` | generic reference classes + hashing (owner decision below) | contract before open |

Two phases outside this file share the queue: **P14 narrow numerics**
(`compiler.md` §16 — a type-system extension, not stdlib) and the
tracked `bindgen` follow-ups. P14 is sequenced first among them
because a production C header with a single `uint8_t` field cannot be
bound until it lands.

**`Map`/`Set` — non-goal reversed (owner decision 2026-07-25.)** They
were listed as a non-goal on the grounds that they need general
generics and that `T[]`/`FixedArray` are the containers. Evidence
against that: game scripts do sparse associative lookup constantly
(entity id → object, asset name → handle) and the only alternative
today is a linear scan or parallel arrays; and the language already
monomorphizes generic functions and generic value classes at check
time (`a12`), with `Array.map`'s `U` inferred from a closure return
(P11), so "general generics" overstates the gap. What is genuinely new
is a **generic reference class with methods plus hashing**; per-kind
key equality is already defined (Q22's `indexOf` rule). The contract
lands here before implementation opens and must state the iteration
order rule (JS `Map`/`Set` are insertion-ordered — determinism, §0.3,
requires pinning it), the key-kind whitelist, and the growth/rehash
policy under the no-implicit-GC memory model.

**Stdlib non-goals** (permanent unless revised with evidence):
`RegExp`, `Intl`/locale- and Unicode-table-dependent behavior
(collation, full case folding — Q21 is ASCII), `Promise` (C8:
coroutines), `console` (the language has `print`), `Symbol`,
`Proxy`/`Reflect`, `eval`/`Function`, `BigInt` (`i64`/`u64` exist).

## 8. P10 — `String` methods

Semantics rule (**Q21**): the language's strings are immutable UTF-8
byte strings; every index, length, and code unit in the accepted
subset is a **byte** measure — the standing meaning of the existing
`length`/`slice`. ASCII-only programs behave exactly as JS; on
non-ASCII text the values diverge from JS's UTF-16 units (recorded,
not hidden). Case mapping and whitespace are ASCII-only. Range and
argument errors **trap** (no NaN/RangeError values).

Accepted members (checker: intrinsic member calls on `Type::Str`;
runtime `sub_rt_str_*`, one implementation, both tiers; every method
returning a string allocates via the Context):

- `indexOf(needle: string, from?: i32): i32` — byte index or −1;
  `from` defaults 0, clamped to `[0, length]` (negative → 0)
- `lastIndexOf(needle: string): i32`
- `includes(needle: string, from?: i32): boolean`
- `startsWith(needle: string): boolean`, `endsWith(needle: string):
  boolean` (the lib's optional position arguments are not accepted)
- `charCodeAt(i: i32): i32` — the byte value 0–255 (Q21; JS returns
  the UTF-16 unit); out of range traps (JS returns NaN)
- `split(sep: string): string[]` — no-match → `[whole]`; adjacent
  separators produce empty strings (JS semantics); an **empty
  separator traps** (byte-splitting would fracture UTF-8 code points)
- `trim/trimStart/trimEnd(): string` — ASCII whitespace
  (space, `\t`, `\n`, `\r`, `\f`, `\v`) only (Q21)
- `repeat(n: i32): string` — `n < 0` traps; `repeat(0)` is `""`
- `padStart(len: i32, pad?: string): string`, `padEnd` — `pad`
  defaults `" "`; byte lengths; already-long-enough → unchanged;
  an empty `pad` with `len > length` traps (JS returns the string
  unchanged for empty pad — divergence recorded in Q21: silent
  non-padding hides bugs)
- `toUpperCase(): string`, `toLowerCase(): string` — ASCII A–Z/a–z
  only (Q21)
- `replace(pat: string, repl: string): string` — first occurrence,
  literal (no regex; `$` in the replacement is **not** interpreted —
  Q21; JS substitutes `$$`/`$&`)
- `replaceAll(pat: string, repl: string): string` — all occurrences,
  literal; empty `pat` traps (JS inserts between every unit)

Rejected (S014, Q21): `substring`/`substr`/`at`/`charAt` (redundant
with `slice`), `codePointAt`, `normalize`, `localeCompare`,
`toLocaleUpperCase`/`LowerCase`, `match`/`matchAll`/`search` (regex),
`concat` (redundant with `+`). `String.fromCharCode`/`raw` and `String`
as a value or constructor are rejected through the standing
unknown-name paths (S100; behavior pinned by unit test — a dedicated
S014 is a follow-up if the diagnostic proves confusing).

Corpus: `a43` string battery — every accepted member incl. the edges:
`indexOf` miss −1 / empty needle 0 / `from` clamp; `lastIndexOf`;
`split` no-match, adjacent separators, trailing separator; `trim`
family boundaries; `repeat(0)`; `pad*` exact/longer/shorter and
two-arg; case round-trip; `replace` vs `replaceAll` multiplicity;
literal `$` in replacement. Rejects: `substring`, `localeCompare`,
`match`, `toLocaleUpperCase` — each S014. Trap paths (`charCodeAt`
OOB, `repeat(-1)`, `split("")`, `replaceAll("", …)`) are cross-tier
cemit tests (identical kind/message/position), not corpus entries.

Gate (pre-registered): standing differential gate byte-exact incl.
`a43`; `tsc` zero errors unchanged config (every accepted call types
under lib ES2022); reject entries at pinned S014 positions; trap
identity across tiers for the four trap paths; §5 item 5 benchmarks
(`specs/blocks/benchmarks.md`) — no ship-row regression.

## 9. P11 — `Array` methods (Q22)

**New machinery: runtime→script closure invocation.** A language
closure is a `(code, env)` pair (C5, non-escaping). Array methods
pass it to the runtime, which calls it synchronously per element
through the language calling convention `(ctx, env, args…)` — the
same shape the P5.2b callback trampoline already invokes. Non-escape
holds by construction (the closure is only called during the method
call). After every callback return the runtime checks the Context
trap flag and stops immediately if set (the trap surfaces through
the standing per-call check in generated code; kind/message/position
identical across tiers — a trapping-callback cross-tier test is part
of the gate).

Accepted members on `T[]` (checker: `ArrFn` intrinsics; runtime
`sub_rt_arr_*`, one implementation, both tiers):

Without closures —
- `indexOf(x)/lastIndexOf(x)/includes(x)`: scalars by value, strings
  by content (`str_eq`), `Date` by millis, reference classes by
  identity (JS `===` semantics per kind)
- `join(sep?): string` — `sep` defaults `","`; elements formatted by
  the Q14 rules (the `${…}` formatting)
- `slice(start?, end?): T[]` — JS negative/clamp rules; fresh array
- `fill(x, start?, end?)`, `reverse()` — in place; return the
  receiver
- `concat(other: T[]): T[]` — exactly one array argument

With closures (callback arities fixed — the lib's optional
index/array parameters are not accepted, Q22) —
- `forEach(f: (v: T) => void): void`
- `map(f: (v: T) => U): U[]` — `U` inferred from the closure return
- `filter(f: (v: T) => boolean): T[]`
- `reduce(f: (acc: U, v: T) => U, init: U): U` — **`init` is
  required** (the lib's no-init overload changes meaning by arity;
  rejected, Q22)
- `some/every(f: (v: T) => boolean): boolean`
- `findIndex(f: (v: T) => boolean): i32` — −1 on miss
- `sort(cmp: (a: T, b: T) => i32)` — **comparator required** (the
  lib's no-argument sort coerces elements to strings — rejected,
  Q22); stable (runtime merge sort); in place; returns the receiver

Rejected (S014, Q22): no-argument `sort`, no-init `reduce`,
`reduceRight`, `find`/`findLast` (a scalar `T[]` has no miss value —
`T | null` does not cover scalars; use `findIndex`), `splice`,
`shift`/`unshift`, `flat`/`flatMap`, `copyWithin`, `entries`/`keys`/
`values`, `forEach`/`map`/… callbacks declaring the index/array
parameters, `every`-family on `FixedArray` (v1 is `T[]` only).

Corpus: `a44` no-closure battery (equality per element kind, join
formatting, slice negatives, fill/reverse/concat); `a45` closure
battery (map type change f64→string, filter, reduce with init,
some/every short-circuit order, findIndex, sort stability — equal
keys keep input order, pinned). Rejects: no-arg `sort`, `find`,
no-init `reduce`, `splice` — S014 each. Trapping-callback cross-tier
test (a callback that traps mid-`map`: identical trap tuple both
tiers) in cemit tests, not corpus.

Gate (pre-registered): standing gate byte-exact incl. a44/a45; `tsc`
zero errors unchanged config; sort-stability pinned; the
trapping-callback tuple identical across tiers; reject entries at
pinned S014 positions; §5 item 5 benchmarks
(`specs/blocks/benchmarks.md`) — no ship-row regression.
