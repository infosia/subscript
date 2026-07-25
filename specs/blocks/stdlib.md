# Standard library — contract

Status: Rev 1, 2026-07-25 (Rev 0: 2026-07-24, P9 `Math`/`Date`; Rev 1 adds the §7 stdlib roadmap and the §8 P10 `String` contract; Rev 2, 2026-07-25, adds the §9 P11 `Array` contract; Rev 3, 2026-07-25, reverses the `Map`/`Set` non-goal and cross-references P14 narrow numerics; Rev 4, 2026-07-25, adds the §10 P15 `Map`/`Set` contract; Rev 5, 2026-07-25, adds the §11 P12 `Number`/parsing/`toFixed` contract; Rev 6, 2026-07-25, moves `toString(radix)`/`toExponential`/`toPrecision`/`Math.clz32` from rejected to accepted per Q26; Rev 7, 2026-07-25, reinstates the thirteen Q27 sweep groups across §1, §8, §9, §10 and §11; Rev 8, 2026-07-26, records Q27 stages 1-3 as implemented and corrects two §12 pre-registrations — no-golden-moves, and which stages touch the checker). Evidence lands in
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

`clz32(x: u32): i32` is accepted (Q26). **`clz32(0)` is `32`** — the
runtime uses Rust's `leading_zeros()` behind an opaque symbol, because
C's `__builtin_clz(0)` is undefined and the ship tier must not emit it.

`imul(a: i32, b: i32): i32` and `fround(x: f64): f64` are accepted
(Q27). Each is an exact duplicate of a spelling the language already
has — `a * b` on `i32`, `x as f32` — and under the owner's rule a
second spelling is not grounds for rejection.

Rejected members (S-code + reject-corpus entries): variadic
`max`/`min`/`hypot` beyond two arguments — the language has no
variadic parameters, which is a missing prerequisite rather than a
cost.

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
`toISOString` round-trips, leap dates, pre-1970). Reject:
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
(collation, locale-sensitive case — Q21 covers non-locale case), `Promise` (C8:
coroutines), `console` (the language has `print`), `Symbol`,
`Proxy`/`Reflect`, `eval`/`Function`, `BigInt` (`i64`/`u64` exist).

## 8. P10 — `String` methods

Semantics rule (**Q21**): the language's strings are immutable UTF-8
byte strings; every index, length, and code unit in the accepted
subset is a **byte** measure — the standing meaning of the existing
`length`/`slice`. Programs whose indices stay in ASCII behave exactly as JS; on
non-ASCII text the values diverge from JS's UTF-16 units (recorded,
not hidden). Case mapping and `trim` whitespace are full Unicode (Q21). Range and
argument errors **trap** (no NaN/RangeError values).

Accepted members (checker: intrinsic member calls on `Type::Str`;
runtime `sub_rt_str_*`, one implementation, both tiers; every method
returning a string allocates via the Context):

- `indexOf(needle: string, from?: i32): i32` — byte index or −1;
  `from` defaults 0, clamped to `[0, length]` (negative → 0)
- `lastIndexOf(needle: string): i32`
- `includes(needle: string, from?: i32): boolean`
- `startsWith(needle: string, position?: i32): boolean`,
  `endsWith(needle: string, endPosition?: i32): boolean` — byte
  offsets (the position arguments were added by Q27)
- `charCodeAt(i: i32): i32` — the byte value 0–255 (Q21; JS returns
  the UTF-16 unit); out of range traps (JS returns NaN)
- `split(sep: string): string[]` — no-match → `[whole]`; adjacent
  separators produce empty strings (JS semantics); an **empty
  separator traps** (byte-splitting would fracture UTF-8 code points)
- `trim/trimStart/trimEnd(): string` — ECMA WhiteSpace + LineTerminator
  (Q21): `U+0009`, `U+000A`, `U+000B`, `U+000C`, `U+000D`, `U+0020`,
  `U+00A0`, `U+1680`, `U+2000`–`U+200A`, `U+2028`, `U+2029`, `U+202F`,
  `U+205F`, `U+3000`, `U+FEFF`. Note `U+0085` (NEL) is **not** in the
  set — Rust's own `trim` would remove it, so the predicate is written
  out rather than delegated
- `repeat(n: i32): string` — `n < 0` traps; `repeat(0)` is `""`
- `padStart(len: i32, pad?: string): string`, `padEnd` — `pad`
  defaults `" "`; byte lengths; already-long-enough → unchanged;
  an empty `pad` with `len > length` traps (JS returns the string
  unchanged for empty pad — divergence recorded in Q21: silent
  non-padding hides bugs)
- `toUpperCase(): string`, `toLowerCase(): string` — Unicode Default
  Case Conversion, including the special-casing table (Q21)
- `replace(pat: string, repl: string): string` — first occurrence, no
  regex. **`$` in the replacement is interpreted** (Q27): `$$`, `$&`,
  `` $` ``, `$'`. `$1`–`$9` stay literal, which is ECMA's own behaviour
  for a string pattern — it has no capture groups — so no regex engine
  is involved
- `replaceAll(pat: string, repl: string): string` — all occurrences;
  empty `pat` traps (JS inserts between every unit)

Added by Q27 (2026-07-25) — all byte-indexed, following Q5:

- `substring(start: i32, end?: i32): string` — **not** `slice`:
  negative arguments clamp to `0` and a reversed pair is swapped, so
  `"hello".substring(-2, 3)` is `"hel"` where `slice(-2, 3)` is `""`
  (measured, node v24.18.0). Off a UTF-8 boundary traps, as `slice`
  does
- `substr(start: i32, length?: i32): string` — negative `start` counts
  from the end; a non-positive `length` gives `""`; boundary trap
- `charAt(i: i32): string` — the code point **starting at byte `i`**;
  out of range is `""`, which is JS's own answer and needs no miss
  value; off a code-point boundary traps
- `codePointAt(i: i32): i32` — the code point starting at byte `i`;
  out of range **traps** (JS returns `undefined`), as `charCodeAt`
  already does; off a boundary traps
- `concat(other: string): string` — one argument, matching `Array`'s
- the position argument of `startsWith(needle, position?)` and
  `endsWith(needle, endPosition?)` — byte offsets

`replace`/`replaceAll` now **interpret `$` in the replacement**,
closing the divergence Q21 recorded: `$$` is a literal `$`, `$&` the
match, `` $` `` the prefix, `$'` the suffix. `$1`–`$9` are **not**
substituted — that is ECMA's behaviour for a string pattern, which has
no capture groups, so this needs no regex engine (verified:
`"a-b".replace("-", "[$1]")` is `"a[$1]b"`).

Rejected (S014, Q21/Q27): `at` — out of range is `undefined` in JS and
there is no miss value for it (`string | null` is itself rejected by
S011); use `charAt`, which is total. `normalize` (Unicode
normalization tables), `localeCompare`,
`toLocaleUpperCase`/`LowerCase` (locale data),
`match`/`matchAll`/`search` (a regex engine) — each a missing
prerequisite rather than a cost. `String.fromCharCode`/`raw` and
`String` as a value or constructor are rejected through the standing
unknown-name paths (S100; behavior pinned by unit test — a dedicated
S014 is a follow-up if the diagnostic proves confusing).

Corpus: `a43` string battery — every accepted member incl. the edges:
`indexOf` miss −1 / empty needle 0 / `from` clamp; `lastIndexOf`;
`split` no-match, adjacent separators, trailing separator; `trim`
family boundaries; `repeat(0)`; `pad*` exact/longer/shorter and
two-arg; case round-trip; `replace` vs `replaceAll` multiplicity; `$`
substitution (**revised by Q27** — the entry previously pinned a
literal `$&`, and closing that divergence moved `a43`'s golden line
`repdollar x=$&` to `repdollar x=1` under the `compiler.md` §2
golden-change procedure; the corpus source's assertion is unchanged).
`a64` covers the rest of the Q27 String surface. Rejects:
`localeCompare`, `match`, `toLocaleUpperCase` — each S014;
`r25-string-substring` was **removed**, `substring` now being accepted.
Trap paths (`charCodeAt` OOB, `repeat(-1)`, `split("")`,
`replaceAll("", …)`, and Q27's `charAt`/`codePointAt` off a UTF-8
boundary and `codePointAt` OOB) are cross-tier
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
  identity. `indexOf`/`lastIndexOf` use JS `===` per kind;
  **`includes` uses SameValueZero** (Q22, revised 2026-07-25) and so
  finds `NaN`, as JS does. The two rules differ in that one case only.
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

Added by Q27 (2026-07-25):

- `reduceRight(f: (acc: U, v: T) => U, init: U): U` — `init` required,
  by the same rule that requires it on `reduce`
- `splice(start: i32, deleteCount: i32): T[]` — **delete-only**,
  returning the removed elements as a fresh array. JS's variadic
  insert form (`splice(1, 2, 9, 9, 9)`) needs variadic parameters,
  which the language does not have; this is a recorded subset, not
  parity
- `shift(): T` — **traps when empty**, exactly as `pop` already does
  (Q4/Q15), so JS's `undefined` never has to be represented
- `unshift(x: T): i32` — **one element**, matching `push`; returns the
  new length. JS's variadic form is the same missing prerequisite as
  `splice`'s
- `copyWithin(target: i32, start: i32, end?: i32): T[]` — JS
  negative/clamp rules, in place, returns the receiver
- **the index parameter on callbacks**: `f(v: T, i: i32)` is accepted
  wherever `f(v: T)` is
- the `every` family on `FixedArray`

Rejected (S014, Q22/Q27): no-argument `sort`, no-init `reduce`
(each changes meaning with arity); `find`/`findLast` (a scalar `T[]`
has no miss value — `T | null` does not cover scalars; use
`findIndex`); `at` (same reason); `flat`/`flatMap` (the depth appears
in the result type, so a runtime depth cannot be typed — undecided
rather than refused, `js-api-sweep.md`); `entries`/`keys`/`values`
(the iterator protocol is not in the language); and the **`array`
parameter on callbacks** — `f(v, i)` passes a value and an index, but
`f(v, i, arr)` hands the callback a reference to the container being
iterated, which is the defect the P15 review found in aggregate
`Map.forEach` and contradicts C5's non-escaping-by-construction rule.

Corpus: `a44` no-closure battery (equality per element kind, join
formatting, slice negatives, fill/reverse/concat); `a45` closure
battery (map type change f64→string, filter, reduce with init,
some/every short-circuit order, findIndex, sort stability — equal
keys keep input order, pinned). Rejects: no-arg `sort`, `find`,
no-init `reduce` — S014 each; `r32` was **repurposed** by Q27 stage 3
from rejecting `splice` outright to rejecting its variadic insert
form. Trapping-callback cross-tier
test (a callback that traps mid-`map`: identical trap tuple both
tiers) in cemit tests, not corpus.

Gate (pre-registered): standing gate byte-exact incl. a44/a45; `tsc`
zero errors unchanged config; sort-stability pinned; the
trapping-callback tuple identical across tiers; reject entries at
pinned S014 positions; §5 item 5 benchmarks
(`specs/blocks/benchmarks.md`) — no ship-row regression.

## 10. P15 — `Map` / `Set` (Q24)

Owner decision 2026-07-25 reversed the non-goal (§7). This is the
stdlib's first **generic reference class with methods**, and its first
**hash container**; the design rules of §0 apply unchanged (one runtime
implementation behind opaque `sub_rt_*`, deterministic, ECMA semantics
for the accepted subset, `tsc` sees the ES2022 lib).

### 10.1 Shape

`Map<K, V>` and `Set<K>` are **reference classes** (heap, Context
memory, manual lifetime — C2's plain-`class` side), monomorphized on
first use exactly as `a12`'s generic value class is. `new Map<K, V>()`
allocates; `unsafeDelete` frees; `collect()` reclaims an unreachable
one. The `K`/`V` monomorphization means there is no boxing and no
type erasure: a `Map<i32, Vec3>` stores `i32` keys and `Vec3` values
inline.

### 10.2 Key kinds (whitelist) — Q24

A key type must have a defined equality **and** a defined hash. The
whitelist is exactly the kinds Q22 already defines equality for:

- sized integers (`i8`…`u64`), `boolean`, `enum` — by value
- `f32`/`f64` — by **SameValueZero** (Q24, revised 2026-07-25), which
  is what JS uses for `Map`/`Set` keys: `NaN` equals itself, so a `NaN`
  key is retrievable and every `NaN` payload is the same key. A literal
  `NaN` key is **accepted** — the earlier rejection existed only
  because the entry would have been unreachable under `===`.
  `+0` and `-0` are one key, and **`-0` normalizes to `+0` on insert**
  so `forEach` reports `0` as JS does.
- `string` — by content, hashed over the UTF-8 bytes
- `Date` — by millis (its erased `i64`)
- reference classes — **by identity** (the handle), never structurally

Rejected as key types (S014, naming Q24): `f16` (storage-only, Q23 —
no arithmetic domain, and its `as f32` widening would make two
distinct `f16` bit patterns collide silently), `T[]`, `FixedArray`,
value classes (`@CStruct` — structural equality is not defined for
them and identity is meaningless for a copied value), `object`
(boundary-opaque), function types, `Nullable<T>`, `void`.

`V` has no whitelist: any type the language can store in a field can
be a value, including value classes and `T[]`.

### 10.3 Iteration order — insertion order, pinned

JS `Map`/`Set` iterate in insertion order; §0.3 requires determinism,
so this is **normative here**, not an implementation accident: entries
iterate in the order they were first inserted; re-assigning an
existing key (`set` on a present key) **keeps its original position**;
`delete` removes it, and re-inserting the same key appends it at the
end. The container therefore carries an insertion-ordered entry vector
alongside its index — a golden pins the order across insert /
overwrite / delete / re-insert.

### 10.4 Accepted API

`Map<K, V>`: `new Map<K, V>()`, `size: i32`, `get(k): V | null`
(reference-class `V`) — see 10.5 for scalars, `set(k, v): Map<K, V>`
(returns the receiver, as the lib does), `has(k): boolean`,
`delete(k): boolean`, `clear(): void`, `forEach(f: (v: V, k: K) =>
void): void` (the lib's third `map` callback parameter is not
accepted, as Q22 fixes callback arities).

`Set<K>`: `new Set<K>()`, `size: i32`, `add(k): Set<K>`,
`has(k): boolean`, `delete(k): boolean`, `clear(): void`,
`forEach(f: (k: K) => void): void`.

Added by Q27 (2026-07-25):

- `Map.groupBy<K, T>(items: T[], f: (v: T) => K): Map<K, T[]>` — `K`
  must be a §10.2 key kind. `Object.groupBy` stays rejected: it
  returns a null-prototype object, which is not a type this language
  has
- ES2024 set algebra on `Set<K>`: `union`, `intersection`,
  `difference`, `symmetricDifference` returning a fresh `Set<K>`, and
  `isSubsetOf`, `isSupersetOf`, `isDisjointFrom` returning `boolean`.
  The argument is a `Set<K>`, not JS's "set-like" duck type, which
  would need a protocol the language does not have. **Result order is
  normative**, as §10.3 requires of all traversal, and is node's:
  receiver order first, then the argument's contribution — `{1,2,3}`
  against `{3,4}` gives union `1,2,3,4`, intersection `3`, difference
  `1,2`, symmetric difference `1,2,4`

Rejected (S014, Q24): the iterator protocol (`keys`/`values`/
`entries`/`for…of`/spread — `forEach` is the traversal) and
construction from an iterable (`new Map([[k, v]])`). Both wait on an
iterator protocol, which the owner has recorded as wanted at high
priority (`js-api-sweep.md`).

### 10.5 The miss problem — `get` on a scalar value type

`get` must report "absent". For a reference-class `V`, `V | null`
carries it (C7). For a **scalar** `V` there is no miss value — the
same problem Q22 solved by rejecting `find` in favour of `findIndex`.
Rule: `get` returns `V | null` **only where `V` is a nullable-capable
type** (reference class, handle); for every other `V`, `get` is
rejected (S014) and the program uses `has` plus a total accessor:

- `getOr(k, fallback: V): V` — this contract's addition, not a lib
  member. It is `tsc`-clean because it is declared in the prelude's
  ambient `Map` augmentation, and it is total for every `V`.

The alternative — returning a zeroed `V` on a miss — is rejected: it
is silently wrong for a program that stores zero as a real value.

### 10.6 Hashing and growth

One runtime implementation, both tiers, behind opaque `sub_rt_map_*` /
`sub_rt_set_*`. Open addressing or bucketed chaining is the
implementer's choice; what this contract fixes is the observable
behaviour:

- The hash function is **the runtime's own**, deterministic and
  seed-free (a per-Context random seed would break the golden corpus
  and replays — §0.3). It is not exposed to script.
- Growth allocates from Context memory and **never runs unbidden**
  (invariant 2): a `set`/`add` may allocate, nothing else does. There
  is no incremental rehash triggered by an unrelated operation.
- A key's hash is a pure function of its value/identity, so the same
  program produces the same iteration order and the same output on
  both tiers — the standing gate checks that byte-for-byte.
- Mutating a reference-class key after insertion does not move it
  (identity hashing), so no rehash hazard exists.

### 10.7 Interaction with existing rules

- **C5**: `forEach` callbacks are non-escaping, like Q22's, and the
  trap flag is checked after every callback return.
- **Invariant 2**: `clear()` and `unsafeDelete` free eagerly;
  `collect()` reclaims an unreachable container **and the keys/values
  it uniquely held** — the container's storage must be scannable by
  the collector (the P2/P4.3 root-range machinery), on both tiers.
- **Hot reload** (compiler block §8.2): `Map`/`Set` are declarations
  only through their type arguments; a `Map<K, V>` whose `K`/`V`
  layout changes is a declaration-hash change and so restarts, which
  is already the rule.

### 10.8 Corpus and gate (pre-registered)

Accept entries (continue the `aNN` numbering): a map battery
(`set`/`get`/`getOr`/`has`/`delete`/`size`/`clear`, integer and string
keys, a value-class value); an **insertion-order** entry pinning
insert / overwrite-keeps-position / delete / re-insert-appends; a set
battery; a reference-class-key entry proving identity semantics (two
equal-shaped instances are distinct keys); and a `forEach` entry
exercising the trap path. Rejects: an `f16` key, a `T[]` key, a
`@CStruct` key, `get` on a scalar-valued `Map`, an iterator-protocol
member, and `new Map([[k, v]])` — each S014 at a pinned position.

Gate: standing gate byte-exact on both tiers including the new
entries; `tsc` zero errors, unchanged config; iteration order pinned
by golden; the collector reclaims a dropped container (observable via
a `collect()` entry that then still prints correctly); a trapping
`forEach` callback reports an identical tuple across tiers; rejects at
pinned S014 positions; benchmarks — no ship-row regression.

## 11. P12 — `Number` statics, `parseInt`/`parseFloat`, `toFixed` (Q25)

No new machinery: these extend the ambient-namespace and member
surfaces P9/P10 already built. §0's rules apply unchanged.

### 11.1 `Number` statics

Constants (folded to `f64` literals at check time, like `Math`'s):
`MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `EPSILON`, `MAX_VALUE`,
`MIN_VALUE`, `POSITIVE_INFINITY`, `NEGATIVE_INFINITY`, `NaN`.

Predicates, all `(value: f64): boolean` with ECMA semantics:
`Number.isNaN`, `Number.isFinite`, `Number.isInteger`,
`Number.isSafeInteger`.

The **global** `isNaN`/`isFinite` are rejected (S014): they coerce
their argument, and coercion is not in this language. `Number.*` is
the spelling.

`MAX_SAFE_INTEGER` describes `f64` integer precision and stays
meaningful as such; it is **not** a bound on `i64`/`u64`, which are
exact 64-bit (C3). A comment in the corpus entry says so, because the
name invites the opposite reading.

### 11.2 The failure channel — why `NaN` here and not elsewhere

Parsing is the first accepted operation whose failure is **data, not a
programmer error**: a config string or a save file may legitimately not
be a number, and the program must be able to carry on. So the trap
model (C6) is wrong here, and so is rejecting the operation.

`parseInt`/`parseFloat` therefore return **`f64`, with `NaN` as the
failure value**, as ECMA defines (§0.4). This does not contradict the
two earlier sentinel rejections, and the difference is the point:

- **Q20 rejected Invalid-Date** because `Date` erases to `i64`, which
  has no NaN — the sentinel would have had to be a magic in-range
  integer, indistinguishable from a real time.
- **Q24 rejected a zeroed `get` miss** because zero is a legitimate
  stored value, so the sentinel collides with real data.
- Here the sentinel is `NaN` in `f64`, where it is **representable,
  outside the value domain of any successful parse, and checkable**
  with `Number.isNaN`. No real result can be mistaken for it.

`parseInt` returns `f64` rather than a sized integer for exactly this
reason: no integer type can carry the failure. The program checks and
then converts (`as i32`), which is the language's existing explicit
conversion rule (C3), not a special case.

### 11.3 `parseInt` / `parseFloat`

- `parseInt(s: string, radix: i32): f64` — **the radix is required.**
  ECMA's default is context-dependent (base 10, except a `0x` prefix
  means 16), and Q22 already rejected two lib forms whose meaning
  changes with arity (`reduce` without `init`, `sort` without a
  comparator) for the same reason. Accepted radixes are 2–36; anything
  else **traps** (it is a programmer error, not data). Otherwise ECMA:
  leading whitespace skipped, optional sign, longest valid prefix
  consumed, `NaN` when no digits are consumed.
- `parseFloat(s: string): f64` — ECMA: leading whitespace skipped,
  longest valid prefix consumed (`"1.5abc"` → `1.5`), `Infinity`
  recognized, `NaN` when no prefix parses.

Prefix parsing is kept rather than tightened: it is what ECMA
specifies and what `tsc` types, and §0.4 makes ECMA the default. A
program that wants strictness checks the string first.

### 11.4 `toFixed`

`toFixed(digits: i32): string` on `f32`/`f64`. `digits` is 0–100;
outside that range **traps** (programmer error). Fixed-decimal output
deliberately differs from Q14's shortest-round-trip — that is the
point of asking for it — so Q14 is unchanged and this is the only
place a numeric string is not shortest-round-trip.

Pinned by golden, because these are exactly where implementations
disagree: half-way cases (ECMA specifies "let n be an integer for
which n / 10^f - x is as close to zero as possible; if there are two
such n, pick the larger" — so `(1.005).toFixed(2)` is `"1.00"`,
because the stored double is below the decimal 1.005), negative zero,
values ≥ 1e21 (ECMA falls back to `ToString`, i.e. the Q14 form — which
since the 2026-07-25 Q14 correction uses ECMA's exponent thresholds, so
`(1e21).toFixed(2)` is `"1e+21"`, matching node),
`NaN` → `"NaN"`, `±Infinity` → `"Infinity"`/`"-Infinity"`, and a
negative value's sign placement.

One implementation behind an opaque `sub_rt_num_*` symbol on both
tiers (§0.2) — never the host libc's `snprintf("%.*f")`, whose
rounding is platform-dependent.

### 11.5 `toString(radix)`, `toExponential`, `toPrecision`

Accepted on `f32`/`f64` (Q26, 2026-07-25). They were rejected in the
first revision of Q25 as "not in v1"; that was a scope statement, and
the owner's standing rule is that a JS API which is implementable at
realistic cost is implemented regardless of expected demand. Measured
cost: about 440 lines total, no external dependency, pure computation.

- `toString(radix: i32): string` — **the radix is required**, for the
  reason §11.3 gives for `parseInt`: an arity that changes meaning is
  what Q22 rejected in `reduce` and `sort`. Radix 2–36; anything else
  **traps**. Radix 10 must agree with Q14 exactly. The fractional part
  is converted too (`(1234.5678).toString(36)` is `"ya.kfv9yqdpm"`),
  which is the substantial part of the implementation. This closes a
  real asymmetry: `parseInt(s, 16)` could read hexadecimal but nothing
  could write it, and the Q14 template form is base 10 only.
- `toExponential(digits?: i32): string` — `digits` 0–100, else traps.
  Omitted `digits` uses as many digits as needed to represent the
  value uniquely.
- `toPrecision(digits: i32): string` — `digits` 1–100, else traps.
  Note the argument is **required** here, unlike JS, where the
  no-argument form is `ToString` — the same arity rule again.

`NaN` and `±Infinity` format as in §11.4 for all three.

One implementation behind `sub_rt_num_*` on both tiers (§0.2), never
libc. §11.4's reason (platform-dependent rounding) applies, and there
is a second: **C's `%e` pads the exponent to two digits where ECMA does
not** — node gives `(0).toExponential(2)` as `0.00e+0`, `printf` gives
`0.00e+00`.

### 11.6 Rejected (S014, naming Q25)

`Number.parseInt(s, radix)` and `Number.parseFloat(s)` are **accepted**
(Q27) with the §11.3 signatures, radix still required. They were
rejected under a one-spelling rule; measured on node, they are the same
function objects as the globals (`Number.parseInt === parseInt`), and a
second spelling is not grounds for rejection.

`Number` as a constructor or a coercing call (`Number(x)`) and the
global `isNaN`/`isFinite` (§11.1) — these **coerce**, which is the
unsoundness the language exists to reject; adding them would import it,
so neither the Q26 nor the Q27 rule reaches them. `toLocaleString`
(needs locale data; `js-alignment-audit.md` records that Boa needs the
same thing, so this is a missing prerequisite, not a cost question).

### 11.7 Corpus and gate (pre-registered)

Accept (continue the `aNN` numbering): a `Number` statics and
predicates battery (including the `MAX_SAFE_INTEGER` ≠ `i64`-bound
comment); a parse battery (success, prefix parse, whitespace, sign,
each radix boundary 2/16/36, `NaN` failure checked with
`Number.isNaN`, then `as i32` conversion of a success); a `toFixed`
battery covering every §11.4 pinned case; and a §11.5 battery —
`toString` at radix 2/8/16/36 over integral and **fractional** values,
negative values, `NaN`/`±Infinity`, radix 10 shown equal to the Q14
template form; `toExponential` with and without `digits`, including
the unpadded-exponent case `(0).toExponential(2)`; `toPrecision`
across the fixed/exponential switchover; and `Math.clz32` at `0`, `1`,
`2^31` and an all-ones input. Rejects: the global `isNaN`, `Number(x)`,
a `parseInt` without a radix, a `toString` without a radix, and a
`toPrecision` without `digits` —
each S014 at a pinned position; plus a radix-out-of-range trap and a
`digits`-out-of-range trap, whose tuples must be identical across
tiers.

Gate: standing gate byte-exact on both tiers including the new
entries; `tsc` zero errors, unchanged config; the `toFixed` and parse
goldens hand-derived from ECMA and cross-checked against node, with
any divergence recorded in Q25 rather than absorbed; trap tuples
identical across tiers; rejects at pinned S014 positions; benchmarks —
no ship-row regression.

## 12. P18 — the Q27 sweep groups: corpus and gate (pre-registered)

**Status: stages 1, 2 and 3 implemented** (`Math`/`Number`, `String`,
`Array`). Stages 4 and 5 — `Map`/`Set` and the callback index
parameter — are still contract only, and the checker still rejects
those members. `generated-docs/api-reference.md` reports the checker,
not this contract (`compiler.md` §17.1), so the two agree on stages
1–3 and disagree on 4–5 by design while P18 is open.

Q27 spans five sections, so its corpus is registered here rather than
split across them. Staged in the order below; each stage is a Phase
Review boundary. **Correction (2026-07-26):** this paragraph said the
last stage touches the checker and the first four do not. That is
wrong — every stage extends the checker's accepted-member tables and
its fixed-arity checking. What is unique to stage 5 is that it needs
**new arity machinery**, a callback being accepted at two arities;
the others only add entries to machinery that already exists.

**Stage 1 — `Math` and `Number` (no new machinery).** `imul`, `fround`,
`Number.parseInt`/`parseFloat`. Accept: an entry pinning `imul`'s
wrapping at the `i32` boundary, `fround`'s rounding
(`Math.fround(1.1)` is `1.100000023841858`), and the two `Number`
statics agreeing with the globals on the same inputs. The existing
`r15`/`r17` reject entries for `imul`/`fround` must be **removed or
repurposed** — they now assert the opposite of the contract.

**Stage 2 — `String`.** Accept: `substring` with a reversed pair and
negative arguments shown differing from `slice` on the same inputs
(this is the entry that proves it is not a duplicate); `substr` with a
negative start and a non-positive length; `charAt` in range, out of
range (`""`), and on a multi-byte code point; `codePointAt` on ASCII
and on a multi-byte code point; `concat`; `startsWith`/`endsWith` with
a position. `$` substitution: `$$`, `$&`, `` $` ``, `$'`, and `$1`
shown **literal**. Traps, tuple-identical across tiers: `charAt` and
`codePointAt` off a UTF-8 boundary, `codePointAt` out of range.

**Stage 3 — `Array`, no new arity machinery.** Accept:
`reduceRight` right-to-left order pinned (a non-commutative fold, so
the direction is observable); `splice` returning the removed elements
and mutating in place; `unshift` returning the new length; `shift`;
`copyWithin` with negative arguments. Traps: `shift` on an empty
array. The existing `r32-array-splice` reject entry must be removed or
repurposed. Rejects to add: the variadic `splice` insert form and
multi-argument `unshift`, each S014 naming variadic parameters as the
missing prerequisite — otherwise a reader cannot tell a subset from an
oversight.

**Stage 4 — `Map`/`Set`.** Accept: `Map.groupBy` over an array with a
`string` key, showing group order and membership; each of the four
set-algebra operations with **result order pinned**, and the three
predicates. Reject: `Object.groupBy`, and a set operation given a
non-`Set` argument.

**Stage 5 — the callback index parameter (checker).** Accept: `forEach`,
`map`, `filter`, `some`, `every`, `findIndex` and `sort` each called
with both arities where the lib allows it, and `reduce`/`reduceRight`
with the index. Reject: the three-parameter `(v, i, arr)` form, S014
naming C5 — this is the narrowing most likely to be read as an
oversight, so its reject entry carries the reason.

Gate: standing differential gate byte-exact on both tiers for every new
entry; `tsc` zero errors, unchanged config; every accept golden
generated from node v24.18.0 and `cmp`-verified, with any divergence
recorded in Q27 rather than absorbed — the `substring`-versus-`slice`
and `$1`-stays-literal lines exist to be checked against node, not
assumed; trap tuples identical across tiers; rejects at pinned S014
positions; benchmarks — no ship-row regression.

**Correction (2026-07-26): this section originally required that no
pre-existing `.expected` move, "since Q27 adds surface and changes
none". That was wrong.** Q27 does change accepted behaviour in one
place: `$` substitution in `replace`/`replaceAll` closes a divergence
Q21 had recorded, so `a43`'s `repdollar` line necessarily moved from
`x=$&` to `x=1`. The rule that matters is the `compiler.md` §2
golden-change procedure — a moved golden must cite the language rule
defining the new bytes and land in the phase tracking file — not a
blanket prohibition. The implementer reported the movement rather than
weakening the corpus to preserve the old bytes, which is the required
behaviour; `a43`'s source assertion is unchanged and only its comment
was updated.
