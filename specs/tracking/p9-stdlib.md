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

## P9.2 — `Date`: COMPLETE (2026-07-24)

The UTC-deterministic subset (stdlib.md §3, Q20): a nominal checker
type (`Type::Date`) erasing to i64 UTC epoch milliseconds; `new
Date(ms)` (TimeClip trap — no Invalid-Date value), `Date.UTC` (ECMA
MakeDay/MakeTime carry incl. negative months/days, MakeFullYear
0–99→1900+, i128 intermediates — extreme i32 args trap, never wrap),
`Date.now()` (Context clock; `sub_rt_ctx_set_now` pins it; default
system UTC, pre-epoch-safe), field-coded `getUTC*` accessors,
`toISOString` (years 0000–9999 else trap; euclidean decomposition —
`-1 ms` is `1969-12-31T23:59:59.999Z`). `getTime` folds at check time
to the i64 receiver (trap order preserved — probe-verified). Both
tiers call identical opaque `sub_rt_date_*` symbols; extern widths
verified against the runtime signatures. Rejections (S014, Q20):
local-time accessors, setters, `parse`, `toString` family, multi- and
zero-argument constructors, `Date` in templates, direct `Date`
comparison (compare `getTime()`); the nominal wall blocks implicit
`Date`↔`i64` both directions; user declarations named `Date` shadow
the builtin in every scope form.

Calendar verification (Phase Review, fresh no-context): 3009
decompose/ISO values and 2017 `Date.UTC` tuples — randomized across
the full ±8.64e15 range plus adversarial edges (negative carries,
MakeFullYear 0/99/100/-1, TimeClip ±1 ms, i32 extremes) — all matched
an independent ECMA-262 implementation; the three trap paths fire
byte-identically on both tiers with identical kind/message/position.

Review: 0 CRITICAL, 3 MAJOR, 2 MINOR. Fixed (`d19e304`):

- MAJOR 1: `new Date` under a *function-local* binding named `Date`
  bypassed the shadow and accepted a program stock `tsc` rejects
  (TS2351, run-verified) — invariant 5. `check_new` now consults
  function-local scopes via the shared `date_is_ambient` helper; a
  shadowed `new Date(...)` is S100. Three scope-form unit tests.
- MAJOR 2: the §3-promised ship-tier pinned-clock `Date.now` test was
  missing (dev-tier only). Added in `cemit.rs`: entry derived from
  `AOT_ENTRY_C` pins the clock via `sub_rt_ctx_set_now` before
  `ss_init`; same program/ms/expected bytes as the dev-tier test.
- MAJOR 3: this entry (with the §5.5 row below).
- MINOR 1: `r24-date-compare` reject entry added; Q20's rejection
  list amended (`0ad8e36`) to record the comparison and zero-arg-
  constructor decisions (corpus-first).
- MINOR 2: r23 header rationale corrected to nondeterminism (a Date
  value is timezone-less UTC millis).

§5 gate evidence: standing gate byte-exact on all 42 entries, both
tiers, goldens byte-unchanged through the fixes; `tsc` zero errors,
config unchanged; a42 (27 lines) + r19–r24 at pinned S014 positions;
calendar unit tests incl. the 1600–2400 full-day round-trip sweep;
§5.5 benchmark at `d19e304` — ship rows unchanged (tree 1.36×,
sort 1.81×, particles 3.06×, compute-bound 0.97–1.03×; run noise
only). `cargo test --offline` 344/0, zero warnings.

## P9 — stdlib v1 (`Math`, `Date`): COMPLETE (2026-07-24)

Both stages complete; the pattern for further stdlib areas is now
standing: the `tsc` side stays lib ES2022, the checker admits a
deterministic sized-typed subset (out-of-subset members S014 with a
Q-register citation), one runtime implementation serves both tiers
through opaque `sub_rt_*` symbols, semantics are pinned by corpus
goldens under the standing differential gate, and nondeterministic
inputs (clock, entropy) are Context-owned and host-settable.

Follow-ups (recorded, not scheduled): S014 wording when calling a
folded constant (`Math.PI(1)` reports S100 — P9.1 MINOR 1); integer
`Math.abs`/`min`/`max` overloads; further areas (`String` methods,
`JSON`, `Array` methods beyond push/pop) each need their own
contract + corpus before implementation.

## P10 — `String` methods: COMPLETE (2026-07-25)

17 byte-measure methods on `Type::Str` (stdlib.md §8, Q21):
indexOf/lastIndexOf/includes/startsWith/endsWith/charCodeAt/split/
trim×3/repeat/padStart/padEnd/toUpperCase/toLowerCase/replace/
replaceAll. `StrFn` intrinsics; check-time optional-arg normalization
(from→0, pad→" "); one runtime implementation (`strops.rs`) behind
opaque `sub_rt_str_*` on both tiers; every string result is a fresh
Context allocation (no interior pointers into the receiver); split
builds `string[]` through the array machinery, element-identical to
literal arrays on both tiers (FFI + cross-tier tests). Four Q21 trap
paths + empty-pad: `TrapKind::StrRange`, kind/message/position
identical across tiers (5 cemit tests). S014 rejects the
out-of-subset lib members; `String` as global/constructor rejected
via the standing unknown-name paths (pinned by unit test).

Corpus a43 (42 lines; golden authored from a JS mirror on the Q21 ≡
JS-on-ASCII rule, except the pinned `$`-literal divergence line) +
r25–r28. Tests 344→376; golden floor 43; tsc clean.

Phase Review (2026-07-25, fresh no-context): 0 CRITICAL, 1 MAJOR,
4 MINOR. Probed 594 edge/randomized ASCII cases against node — zero
divergence beyond the two Q21-pinned ones; non-ASCII bytes pass
through case/trim untouched; every user i32 input clamp/try_from-
guarded (no negative→usize bug); `String` shadowing safe in all
scope forms (dispatch is on the receiver type — the P9.2 Date-shadow
class does not recur).

- MAJOR 1 (this entry): §5.5 benchmark at `58c0a1a` — ship rows
  unchanged (tree 1.37×, sort 1.77×, particles 3.07×, compute-bound
  0.93–1.05×; run noise only).
- MINOR 1: this tracking entry.
- MINOR 2 (fixed with this commit): §8 wording aligned — `String`
  as value/constructor is S100 via standing paths, not S014.
- MINOR 3 (follow-up, recorded): a >2 GiB string constructed via
  `repeat`/`pad*` wraps i32 byte-length measures (pre-existing Q5
  i32 convention; P10 adds the first easy constructors). Close by
  trapping StrRange when a result would exceed i32::MAX bytes.
- MINOR 4 (recorded): host-heap OOM in the strops intermediate Vecs
  aborts (the documented FFI-boundary exception); Context-side
  allocation failure traps.

## P10 follow-ups (not scheduled)
- StrRange trap for results > i32::MAX bytes (MINOR 3).
- Dedicated S014 for `String` as global if S100 proves confusing.

## P11 — `Array` methods: COMPLETE (2026-07-25)

16 methods on `T[]` (stdlib.md §9, Q22) — the project's first
**runtime→script closure invocation**. `ArrFn` intrinsics; one runtime
implementation (`arrops.rs`) behind opaque `sub_rt_arr_*` on both
tiers; type-tag marshaling with per-tier width dispatch (boolean 1 B
JIT / 4 B ship-C, each derived from that tier's own element width, so
correct by construction rather than by coincidence); callback ABI
`(ctx, env, args…)` per shape; the Context trap flag checked after
every callback return in all eight loops; `sort` = stable merge on
scratch with write-back only on completion, so a comparator trap
leaves the input byte-identical. `reduce` requires `init`, `sort`
requires a comparator, `find`/`splice`/`flat`/… are S014 (Q22);
`includes` uses `===` (never finds `NaN`), the one recorded JS
divergence. Corpus a44 (no-closure battery) + a45 (closure battery,
sort stability pinned) + r29–r32; golden floor 45.

Phase Review (2026-07-25, fresh no-context, execution-verified):
**1 CRITICAL, 1 MAJOR, 7 MINOR.**

- **CRITICAL 1 — ship-C evaluated the receiver *after* the arguments**
  for 10 of the 16 methods (`cemit.rs` bound the receiver as an
  expression string, then emitted argument temps as statements), while
  the dev JIT evaluates the receiver first. `mkArr().indexOf(mkNeedle())`
  printed `RN:1` under dev-JIT and `NR:1` under ship-C — **different
  output bytes for a program the language and `tsc` both accept**, and
  invisible to the corpus (a44/a45 use plain identifier receivers).
  Reproduced independently by the orchestrator before the fix and
  confirmed agreeing after.

  Fixing it exposed that the same class was **already live elsewhere**,
  not merely latent: `push` was genuinely inverted, and the sweep found
  two defects worse than ordering — `&&`/`||` hoisted the right
  operand's statements so the **skipped side ran** (a short-circuit
  violation), and compound assignment **evaluated its target base
  twice**. All closed structurally with one primitive (`eval_operands`:
  each operand lowered into its own buffer, pinned to a temporary only
  when a later operand emits statements), applied to call arguments,
  indirect calls, `Math`/`Date.UTC`/`Str` argument lists, binary
  operands, indexing, assignment targets, method receivers, foreign
  calls and boundary-struct initializers. Programs whose operands are
  plain expressions emit byte-identical C, and a22's ship assembly is
  unchanged. Sites verified already correct (unchanged): `eval_cond`,
  templates, dynamic-array literals, `switch` discriminant, value-class
  rvalue receivers. **No dev-JIT deviation from left-to-right found** —
  the dev tier is the reference everywhere.

- **MAJOR 1 — the phase had no tracking entry and its pre-registered
  benchmark item was unmeasured.** This entry closes it; benchmark
  below.

- MINOR fixed: `reduce`'s `init` was checked with no contextual type, so
  a non-`i32` accumulator failed and the diagnostic blamed the callback
  (now takes `U` from the callback per C4, and reports against the
  init); `elem_eq` returned 0 for widths outside {1,4,8} instead of
  trapping (now guarded like the callback path); `push`/`pop` on
  `FixedArray` was blamed on Q22 although `push` is deliberately outside
  the Q22 set (restored to the standing diagnostic); the ambiguous
  "§5.5" citation is now "§5 item 5".
- MINOR recorded, unfixed: a29-style pure-lifecycle observation is
  weak; the mirror-ingestion predicate is a substring match (safe both
  directions).

**Pins added** so the review's verified-but-untested properties cannot
regress silently (32 cross-tier tests total, 407 → 439): trapping
callbacks for all seven remaining closure methods; growth-during-
iteration (a callback pushing past capacity while the runtime iterates —
`read_elem` re-resolving the data pointer per element is what prevents
a use-after-free); `fill`/`reverse`/`sort` returning the receiver, not
a copy; `join` of `-0` (`0.1,2.5,-0` per Q14; node prints `0`); and one
per evaluation-order site class, each verified failing before its fix.

Verified positively by the review, no finding: callback ABI agreement
across tiers for every shape × every element kind the type system
admits; identical trap tuples from all eight loops; `sort`'s
byte-identical input after a comparator trap; no use-after-free under
callback-driven growth; semantics line-for-line with node except the
Q22-pinned `includes`; all 24 rejected members actually rejected.

**Gate (§9, all met):** standing gate byte-exact incl. a44/a45 on both
tiers; `tsc` zero errors unchanged config; sort stability pinned;
trapping-callback tuple identical across tiers; r29–r32 at pinned S014
positions; benchmarks re-captured at `568293b` — **ship rows unchanged**
(tree 1.37×, sort 1.77×, particles 3.06×, compute-bound 0.96–1.02×),
and the new short-circuit lowering costs nothing on the `&&`-heavy rows
(primes 0.96×, queen 1.00×). Orchestrator-verified: 439 tests, 0
failures, zero warnings, goldens untouched.

## P11 follow-ups (not scheduled)
- a29's `ok` marker discriminates the handle lifecycle only weakly.
- Mirror-ingestion predicate is a `subDevice` substring match.
