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

## P15 — `Map` / `Set`: COMPLETE (2026-07-25)

`Map<K, V>` and `Set<K>` per `stdlib.md` §10 / Q24 — the project's first
**generic reference class with methods** and its first **hash
container**. Monomorphized on first use like `a12`'s generic value
class, so keys and values are stored unboxed. One runtime
implementation (`runtime/src/assocops.rs`) behind opaque
`sub_rt_map_*`/`sub_rt_set_*` on both tiers. Corpus `a51`–`a56`,
rejects `r38`–`r45`.

Contract points worth restating because they were decided here, not
inherited: **iteration order is insertion order and is normative**
(§0.3 determinism and the goldens depend on it — overwrite keeps
position, delete-then-reinsert appends); the hash is the runtime's own
and **seed-free** (a per-Context seed would make order, goldens and
replays irreproducible); `get` returns `V | null` only where `V` is
nullable-capable and is otherwise rejected in favour of `has` plus the
total **`getOr(k, fallback)`** — returning a zeroed `V` on a miss was
explicitly rejected as silently wrong for a program that stores zero,
the same reasoning that rejected `find` in Q22.

## Phase Review (2026-07-25, fresh no-context, different model from the
## implementer)

Implementation by Codex `gpt-5.6-sol`; review by an independent
no-context agent. **1 CRITICAL, 1 MAJOR, 9 MINOR** — all fixed.

- **CRITICAL 1 — `Map.forEach` over an aggregate `V` diverged across
  tiers and broke C2.** The dev-JIT bridge passed the raw pointer into
  the map's live entry storage as the callback's aggregate argument,
  while ship-C copied. A callback assigning `v.x = 777` over a
  `Map<i32, V3>` left the container mutated under dev-JIT and untouched
  under ship-C (orchestrator-reproduced). Two defects, one cause:
  dev-JIT ≢ ship-C-AOT on an accepted program (§11), and the dev tier
  letting a callback silently mutate a stored value, which C2 forbids.
  §10.1's own example (`Map<i32, Vec3>`) was exactly the broken shape.
  New in P15 — P11's `Array.forEach` rejects value-class elements
  (Q22), so the array bridge never reaches the aggregate arm. Fixed by
  copying into a caller-owned temporary before the indirect call,
  matching the C emitter. **The gate could not see it**: `a55` used
  only `i32`/`string` values; `a56` now covers `@CStruct` and
  `FixedArray` values, for both a callback that mutates its parameter
  and one that overwrites the entry it is visiting.
- **MAJOR 1 — the insertion-ordered entry vector never compacted.**
  `delete` zeroed the slot and left it occupied, so 200 000
  `set`+`delete` pairs on a map whose live size is always 0 retained
  8.4 MB, unreclaimable by `collect()` (the entries block is reachable
  from the live header), and made `rehash`/`forEach` O(total inserts
  ever). This defeated the very use case §7 cited to justify the phase.
  Fixed by compacting in insertion order before growth, **suppressed
  while an iteration is active** so the mutation-during-`forEach`
  semantics the review had verified node-identical are preserved.
  Orchestrator-verified: after the same 200k churn, `order_len` is 4
  (was 200 000) and the subsequent order still matches node exactly.
- MINOR fixed: golden floor raised to cover the new entries; the
  nested `in_assoc_key` leak that let boundary-only `object` escape
  through a nested container's value type; `Map`/`Set` rejected as key
  kinds for consistency with `T[]` (both are identity handles §10.2
  never listed as accepted) — pinned by `r44`; Q6's dev-tier
  use-after-delete trap added to the container entry points; `get_or`
  made total on a null handle.

**Verified positively by the review, no finding:** memory safety —
every GC route requested (reference-class keys and values, runtime-built
strings, containers in a local, a global, a class field, another
container, a `T[]`, a `FixedArray`) survives `collect()` plus churn on
both tiers, and dropping the root reclaims the container, its entries,
its buckets and all uniquely-held values. Insertion order survives five
entry-vector growths and five rehashes and is **byte-identical to
node**. Key semantics match node except the recorded Q22/Q24 `NaN`
divergence. The hash has no seed (identical output across five
processes). Mutation during `forEach` is defined, identical across
tiers, and node-identical. No memory-unsafe route on either tier.

## The benchmark gate earned its place here

§10.8's pre-registered "no ship-row regression" was the **only** check
that caught the next defect: every test was green, `tsc` was clean, the
standing gate was byte-exact, and the Phase Review had found nothing
further — but the `tree` ship row had moved **1.37× → 1.71×**
(89.7 ms → 111.6 ms), reproduced twice on a quiet machine.

Cause: P15 put Map/Set class resolution **inside `Context::delete`**,
so every delete performed a container-path lookup before the existing
release lookup. `tree` frees 30 × 131 071 nodes — ~3.93 M deletes —
so a program that never touches a container paid a Map/Set tax on its
hottest path. Fixed by folding class resolution into the existing
release lookup: an ordinary delete completes in one lookup and only a
confirmed container enters cleanup. Orchestrator-re-measured: **tree
1.39× (90.6 ms)**, level with the pre-P15 1.37× (89.7 ms); every other
ship row unchanged (sort 1.82×, particles 3.07×, compute-bound
0.97–1.03×). A structural guard now asserts an ordinary delete skips
the container path and that ship-tier delete performs exactly one
membership lookup — a benchmark number alone would not prevent
recurrence.

## Gate (§10.8, all met — orchestrator-verified)

Standing gate byte-exact on both tiers including `a51`–`a56`; `tsc`
zero errors with unchanged config; iteration order pinned by golden and
cross-checked against node; the collector reclaims a dropped container;
a trapping `forEach` callback reports an identical tuple across tiers;
rejects at pinned S014 positions; benchmarks re-captured with no
ship-row regression. 481 tests, 0 failures, zero warnings, no
pre-existing golden modified.

## P12 — `Number`, parsing, `toFixed`: COMPLETE (2026-07-25)

`Number` constants and the four `is*` predicates, `parseInt` with a
**required radix**, `parseFloat`, and `toFixed` — per `stdlib.md` §11 /
Q25. No new machinery: these extend the ambient-namespace and member
surfaces P9/P10 built. One runtime implementation (`runtime/src/num.rs`)
behind opaque `sub_rt_num_*` on both tiers; the host libc's
`snprintf("%.*f")` is deliberately unused (§0.2 — its rounding is
platform-dependent), verified by the review to be absent from the
runtime, the emitter and the emitted C.

Contract point decided here: **`NaN` is admitted as a failure value**,
the only sentinel in the language. Parse failure is *data* — a config
string may legitimately not be a number — so C6's trap model is wrong
for it, and `parseInt` returns `f64` rather than a sized integer
because no integer type can carry the failure. That is exactly what was
missing when Q20 rejected Invalid-Date (`Date` erases to `i64`, which
has no NaN) and when Q24 rejected a zeroed `get` miss (zero is a
legitimate stored value): here the sentinel is representable, outside
the domain of any successful parse, and checkable with `Number.isNaN`.
`parseInt`'s radix is required because ECMA's default is
context-dependent — the same arity-changes-meaning hazard Q22 rejected
for `reduce` and `sort`.

## Q14 corrected in this phase (founding rule)

Codex stopped mid-implementation to report that `(1e21).toFixed(2)` is
`"1e+21"` in node while the contract said it falls back to the Q14
form. That was the tip of a defect in **Q14 itself**: the rule's
"without a decimal point or exponent" wording was aimed at printing `7`
rather than `7.0`, but taken literally it banned exponents at the
magnitude extremes too — so `${5e-324}` produced a **751-character**
string and `${1e21}` diverged from every JS engine, with no divergence
recorded (which §0.4 requires). Owner decision: adopt ECMA's
thresholds, exponential outside **`[1e-6, 1e21)`**. This also makes
`toFixed`'s ECMA-specified fallback coherent instead of contradicting
the interpolation form for the same value.

Blast radius was measured before the change (one golden line) and
confirmed after: **exactly one frozen golden moved** — `a49`'s f16
subnormal, `0.00000005960464477539063` → `5.960464477539063e-8` — under
the `compiler.md` §2 procedure.

## Phase Review (2026-07-25, fresh no-context, different model from the
## implementer)

Implementation by Codex `gpt-5.6-sol`; review by an independent
no-context agent. **1 CRITICAL, 1 MAJOR, 4 MINOR.**

- **CRITICAL 1 — the orchestrator wrote a false claim into the
  normative spec.** Q25 recorded that `(-0).toFixed(d)` *keeps* the
  sign (`-0.00`) where ECMA drops it, with a justification. The
  implementation, its unit test, its doc comment and the `a59` golden
  all did the opposite — ECMA's — and node agrees with them over
  650 588 cases with zero divergences. The claim came from reading the
  golden line `signs 0.00 -0.00 -12.340` and *assuming* its three inputs
  were `0`, `-0`, `-12.34`; the entry's actual middle value is
  `(-0.0001).toFixed(2)`, which is `-0.00` in every engine. Four
  artifacts were right and the spec was wrong. **Resolved by correcting
  the spec**: `toFixed` follows ECMA on `-0`, deliberately unlike Q14's
  interpolation rule, because `${x}` is the language's only
  general-purpose number-to-string path (losing the sign there would
  discard information with no alternative) while `toFixed` is a
  specific formatting request with ECMA-defined semantics. The
  misrecording is itself noted in Q25 so the failure mode stays visible.
  **Rule taken from this: never infer a corpus entry's inputs from its
  golden — read the source.**
- **MAJOR 1 — exact decimal ties break away from zero, not to even**
  (pre-existing, not introduced here). The digits come from Rust's
  shortest-round-trip writer; ECMA breaks ties to even. Measured:
  **339 divergences in 3 010 916 `f64` bit patterns** (0.011 %), all of
  this one class. Both spellings round-trip and **both tiers agree
  byte-for-byte**, so determinism and the standing gate are unaffected;
  only JS agreement is. Recorded in Q14 as a divergence rather than
  fixed: matching ECMA needs a custom shortest-float writer with
  tie-to-even, and a hand-rolled one is a worse risk than the 0.011 %
  it would close. Follow-up.
- MINOR: `parseInt` is **1 ulp more precise than node** at radix
  3/35/36 (ECMA-262 §19.2.5 permits approximation exactly there, and
  there are zero divergences where it requires exactness) — recorded in
  Q25; `parse_float`'s non-UTF-8 internal trap carried `pos_id` 0 while
  every sibling carried a real position — fixed, with a regression test;
  the P12 commit message says "r46-r49" where the corpus adds **r46–r50**
  (`r50-parse-int-no-radix` is the §11.7-required entry — the corpus is
  right, the message undercounts); this tracking entry and the
  benchmark row were the outstanding §2/§11.7 items.

**Verified clean by the review** (~4.06 M values, both tiers): the Q14
correction is exactly node's notation everywhere, perturbs **zero**
in-range values, and is coherent for `f32` as well as `f64`; `toFixed`
matches node over 650 588 cases including the half-way rule
(`(1.005).toFixed(2)` → `"1.00"`), `digits` 0 and 100, `≥1e21`, `NaN`,
infinities and `f32` receivers; `parseFloat` matches over 80 094 cases
including every ECMA whitespace class; all six trap tuples are
identical across tiers; the whole §11.6 rejected surface is S014 with a
Q25 citation, and program-declared shadowing of `parseInt`/`isNaN`/
`Number` correctly wins over the intrinsic. No cross-tier divergence
anywhere.

## Gate (§11.7, all met — orchestrator-verified)

Standing gate byte-exact on both tiers (floor 56 → 59); `tsc` zero
errors, unchanged config; the `toFixed` and parse goldens hand-derived
from ECMA and cross-checked against node with every divergence recorded
in Q14/Q25 rather than absorbed; trap tuples identical across tiers;
rejects at pinned S014 positions; benchmarks re-captured — **no
ship-row regression** (tree 1.39×, sort 1.77×, particles 3.08×,
compute-bound 0.97–1.03×). 493 tests, 0 failures, zero warnings, only
`a49` moved among pre-existing goldens.

## P12 follow-ups (not scheduled)

- ECMA tie-to-even in the shortest-float writer (MAJOR 1).

## P18 stages 1–2 — Q27 `Math`/`Number` and `String` (2026-07-26)

Implemented: `Math.imul`, `Math.fround`, `Number.parseInt`/`parseFloat`
(sharing the globals' implementation rather than forking it), and the
whole Q27 `String` group — `substring`, `substr`, `charAt`,
`codePointAt`, `concat`, the `startsWith`/`endsWith` position
arguments, and `$` substitution in `replace`/`replaceAll`. All
byte-indexed, per Q5. Stages 3–5 (`Array`, `Map`/`Set`, callback
arity) untouched; `r32-array-splice` deliberately left in place.

Corpus: `a63-q27-math-number`, `a64-q27-string`. Both goldens
generated from node v24.18.0 and `cmp`-verified by the implementer,
then **re-verified independently by the orchestrator** on a
hand-written equivalent script — both matched. Reject entries `r15`
(`imul`), `r17` (`fround`) and `r25` (`substring`) removed: each
asserted the opposite of the new contract. The generated API reference
index was used to confirm no other reject entry covered an accepted
member.

`a64` proves `substring` is not a duplicate of `slice` by printing both
on the same inputs: `substring(4, 1)` is `ell` (arguments swapped)
where `slice(4, 1)` is empty, and `substring(-2, 3)` is `hel`
(negatives clamped to 0) where `slice(-2, 3)` is empty.

### Golden change — `a43-string`, under `compiler.md` §2

One frozen golden moved:

```
repdollar x=$&   ->   repdollar x=1
```

**Language rule defining the new bytes:** Q27's `$` substitution — `$&`
expands to the matched substring, so `"x=1".replace("1", "$&")` is
`"x=1"`. This *closes* the divergence Q21 recorded ("`$` in the
replacement is not interpreted"), so the movement is the point of the
change rather than a side effect. Verified against node v24.18.0
directly: `repdollar x=1`.

The corpus source's assertion is unchanged — only its comment was
updated. This matters because the same handoff constraint ("stop if a
pre-existing `.expected` changes") had earlier caused the Q22/Q24 work
to *weaken* `a44` and `a53` so their old goldens would still pass,
which had to be undone. The constraint was restated for this task as
"report it, do not alter the corpus source", and the implementer did.

### Contract correction found by the work

`stdlib.md` §12 pre-registered "no pre-existing `.expected` moves,
since Q27 adds surface and changes none". That was wrong: Q27 changes
accepted behaviour in exactly one place, `$` substitution. The section
now defers to the §2 golden-change procedure instead of prohibiting
movement outright.

### Gate

Zero build warnings; `cargo test` clean including the standing
dev-JIT ≡ ship-C-AOT ≡ golden gate, the P16 structural tests and the
21-witness node run; `tsc` exit 0; `git diff --check` clean. The P16
divergence-witness set changed as designed: the Q21 `$`-substitution
witness was **removed** (the divergence no longer exists, and the
witness test asserts `subscript != node`, so a stale one fails loudly)
and a Q27 `codePointAt` out-of-range witness added.

## P18 stage 3 — Q27 `Array` (2026-07-26)

Implemented: `reduceRight` (required `init`), delete-only `splice`,
`shift`, single-element `unshift`, `copyWithin`. Stages 4–5
(`Map`/`Set`, callback arity) untouched.

Corpus `a65-q27-array`. Golden generated from node v24.18.0 by the
implementer and **re-verified independently by the orchestrator** on a
hand-written equivalent — matched. **No pre-existing `.expected`
moved** (64/64 hashes identical), as expected: unlike stage 2, this
stage changes no accepted behaviour.

`shift` on an empty array traps with the same tuple on both tiers
(`EmptyPop`, `"shift() on an empty array"`), reusing `pop`'s existing
path rather than inventing a second empty-container rule.

Two properties the entry pins that the contract only implies:

- **`copyWithin` returns the receiver, not a copy.** `a65` mutates the
  returned array with `fill` and prints both, showing they alias.
- **`splice`'s required `deleteCount` past the end is clamped**, not
  an error: `[1,2,3].splice(1, 99)` removes `2,3`.

The subset is visible rather than implied: `r32-array-splice` was
repurposed from rejecting `splice` outright to rejecting its variadic
insert form, and `r51-array-unshift-variadic` added, **both with an
S014 message naming variadic parameters as the missing prerequisite**
— §12 requires that, because a reader cannot otherwise tell a
deliberate subset from an oversight.

### Contract correction found by the work

§12 said "the last stage touches the checker and the first four do
not". Wrong: every stage extends the checker's accepted-member tables
and fixed-arity checking. What is unique to stage 5 is **new arity
machinery** — one callback accepted at two arities. Corrected.

### Gate

Zero build warnings; `cargo test` clean including the standing
dev-JIT ≡ ship-C-AOT ≡ golden gate, the P16 structural and witness
tests; `tsc` exit 0; `git diff --check` clean.

## P18 stage 4 — Q27 `Map`/`Set` (2026-07-26)

Implemented: `Map.groupBy` and the ES2024 set algebra (`union`,
`intersection`, `difference`, `symmetricDifference`, `isSubsetOf`,
`isSupersetOf`, `isDisjointFrom`). Stage 5 (callback arity) untouched.

Corpus `a66-q27-map-set`, golden generated from node v24.18.0 and
**re-verified independently by the orchestrator** — matched. No
pre-existing `.expected` moved.

`T[]` is a reference shape, so §10.5 permits `Map<K, T[]>.get`; the
entry uses it rather than `getOr`.

Aggregate ownership was handled explicitly, the P15 defect class being
the reason: group arrays and set results are fresh storage, aggregates
are copied before the callback sees them, and inputs and outputs are
GC roots for the duration. `a66` demonstrates it — mutating
`s1.union(s2)` afterwards leaves `s1` and the source array unchanged.

### Contract correction: `intersection` is not receiver-ordered

Q27 and §10.4 both stated that all four set-algebra operations produce
**receiver order first**. That is wrong for `intersection`, which
iterates the **smaller** set, ties going to the receiver.

The error is instructive: the claim was generalized from a single
measurement, `{1,2,3}` against `{3,4}` yielding `3` — a case where
both candidate rules give the same answer, so it could not
discriminate. Measured with a case that can:

```
{5,4,3,2,1}.intersection({1,3})  ->  1,3     (receiver-first would be 3,1)
{9,8,7}.intersection({7,8,9})    ->  9,8,7   (equal sizes: receiver wins)
```

Consequence worth stating: `intersection`'s output order depends on
the operands' relative sizes. Still deterministic — sizes are — so
§0.3 holds, but it is not what the other three do. `a66` pins both
directions and a smaller-argument case.

### Contract correction: §10.6's allocation list

§10.6 said "a `set`/`add` may allocate, nothing else does". Q27 added
operations that produce fresh containers, so the list is now
`set`/`add`, `Map.groupBy`, and the four algebra operations.

### Gate

Zero build warnings; `cargo test` clean including the standing
dev-JIT ≡ ship-C-AOT ≡ golden gate and the P16 structural and witness
tests; `tsc` exit 0; `git diff --check` clean.

## P18 stage 5 — Q27 callback index parameter (2026-07-26). P18 COMPLETE

Implemented: `forEach`, `map`, `filter`, `some`, `every`, `findIndex`
accept both `(v: T)` and `(v: T, i: i32)`; `reduce`/`reduceRight`
accept the trailing index. The arity flag is carried from both tiers
into the shared runtime, so one implementation serves both.

Corpus `a67-q27-array-callback-index` (accept),
`r55-array-callback-container` (reject). Golden generated from node
v24.18.0 and **re-verified independently by the orchestrator** —
matched. No pre-existing `.expected` moved (66/66).

Every index in `a67` **affects the output**; a callback that ignored
`i` would prove nothing. `reduceRight` prints the indices in callback
visit order, pinning the downward count `3,2,1,0`.

The three-parameter `(v, i, arr)` form is S014, and the diagnostic,
the reject entry and the generated reference all carry the reason:
`f(v, i)` passes a value and an integer, `f(v, i, arr)` passes a
reference to the container being iterated, against C5 and the P15
defect class.

The generated reference renders the dual arity as a union type
(`((value: T) => void) | ((value: T, index: i32) => void)`), so §17's
accepted table stays honest about what the checker takes.

### Scope checks the stage made, rather than assumed

- **`sort` takes no index.** Verified on node: the comparator receives
  exactly two arguments. §12 had listed `sort` in this stage's corpus,
  which was wrong; corrected.
- **`Map.forEach`/`Set.forEach` are unchanged.** JS's `Map.forEach` is
  `(value, key, map)`; the accepted shape here stays the fixed
  `(value: V, key: K) => void`, and `Set.forEach` stays `(key: K)`.
- **`Map.groupBy`'s callback is unchanged.** node passes `(value,
  index)`; §10.4's contract says `(value: T) => K` explicitly, so the
  stage did not widen it. Recorded as a **known narrowing**, not an
  oversight: if the index is wanted there, it is a contract change.

### P18 outcome

All five stages of Q27 are implemented. Five contract claims were
disproved by the implementations and corrected in place: §12's
no-golden-moves pre-registration, §12's claim that only the last stage
touches the checker, §12's inclusion of `sort` in the index stage,
§10.4's receiver-first ordering for `intersection`, and §10.6's
allocation list.

**The pattern is worth one line:** every one of the five was written
before implementation and asserted something no measurement had
discriminated. Contract-first ordering is retained — it is what makes
the handoffs checkable — but a pre-registration is provisional until
an implementation exercises it, and a rule generalized from a single
example that both candidate rules satisfy is not a measurement.

**Phase Review is pending** and required before P18 is marked COMPLETE
in the plan.

## P18 Phase Review (2026-07-26) — findings and disposition

Fresh no-context review of `f51d480..c1a2f5a` (P18's five stages plus
P16). **CRITICAL: none. MAJOR: 3. MINOR: 4.** The review independently
regenerated all five goldens `a63`–`a67` on node v24.18.0 (all match),
regenerated `api-reference.md` (byte-identical), and confirmed the
tracking file's claims about shared `parseInt` identity, `shift`
reusing `pop`'s trap, `copyWithin` returning the receiver, `splice`'s
clamped `deleteCount`, and `groupBy`/set-algebra storage ownership. It
specifically looked for and did **not** find cross-tier divergence,
aggregates escaping into callbacks, emitted-C UB, or a weakened corpus.

### MAJOR 1 — `String.slice` changed from trapping to JS clamping, unrecorded

The stage-2 implementer replaced `sub_rt_str_slice`'s out-of-range
trap with JS's negative/clamp rules so `a64` could print `substring`
and `slice` on the same inputs. The trap message
`"slice(…) out of range for string length …"`, present at `f51d480`,
no longer exists anywhere. No test covered that path, so the suite did
not notice. Worse, this tracking file then described
`slice(-2, 3)` as empty *as if it had always been* — the value it
compares `substring` against was produced by the same commit.

**Disposition: kept and recorded**, not reverted. It is what node does,
and §9 already specified "JS negative/clamp rules" for `T[].slice`, so
string `slice` trapping while array `slice` clamped was an
inconsistency inside one language. The cost is stated in `stdlib.md`
§8 rather than hidden: an out-of-range `slice` used to be an early
error and is now silent, a step away from invariant 6. `collisions.md`
Q5 amended. **This is P18's second accepted-behaviour change**, the
first being `$` substitution.

The general lesson, which the earlier `a44`/`a53` episode already
taught once: a corpus entry that compares a new member against an
existing one is only evidence if the existing one did not move.

### MAJOR 2 — Q27 declared complete with one contracted group unimplemented

`stdlib.md` §9 and `collisions.md` Q27 both list "the `every` family on
`FixedArray`" among the thirteen reinstated groups. The checker still
rejects it (`check/expr.rs`, any `FixedArray` receiver with an
`arr_method` name → S014 under Q22), and `api-reference.md` correctly
documents it as rejected. §12's five stages **never registered a corpus
stage for it**, so the pre-registered gate could not have caught the
omission — the same class of error as the other five §12 defects: a
pre-registration asserting something no measurement checked.

**Disposition: implement as stage 6.** The "fully implemented" claims
in `collisions.md`, `stdlib.md` §12 and this file are withdrawn until
it lands.

### MAJOR 3 — the pre-registered benchmark gate was not run

§12 pre-registered "benchmarks — no ship-row regression" and no stage
entry reported one. Run now, twice:

- First run **void by the harness's own noise check** (C's spread
  exceeded ±20% of its median; `compiler.md` §9 requires the redo).
- Second run, `--warmup 12 --timed 15`, noise check passed:
  emitted-C **1.87x** of the hand-written C baseline; Cranelift
  ship-AOT 22.76x and dev-JIT 26.17x, both the known superseded rows
  (CLAUDE.md: Cranelift AOT was ~23x, which is why the ship tier moved
  to C emission).

**The gate does not test P18.** `perf-gate`'s only subject is
`a22-matrix-propagation`, which uses value structs, fixed arrays and a
hand-written loop — **no array callback method at all**. Nor does any
`benchmarks/workloads/subscript/` entry: `sort.ts` implements
quicksort by hand rather than calling `Array.sort`. So the code stage 5
changed — the per-element `if indexed` branch in `call_value` and
`call_reduce`, on every `forEach`/`map`/`filter`/`some`/`every`/
`findIndex`/`reduce` — **is executed by no benchmark in the
repository**. The pre-registration was satisfied formally and is
evidence of nothing about this phase.

**Two open items follow, neither blocking P18:**

1. **Benchmark coverage gap.** The most-used stdlib surface has no
   benchmark subject. A regression there is currently invisible. A
   workload exercising array callbacks would close it.
2. **Unattributed drift in the emitted-C figure.** 1.87x here against
   **1.05x** recorded for arm64 in `windows-portability.md`. It cannot
   be attributed to P18, since a22 does not execute P18's code, and
   this machine has been under continuous compile load all session.
   It needs an idle-machine re-measurement against a same-session
   baseline build before anything is concluded.

### MINOR 1–4

Recorded and queued with the stage-6 work: `js-api-sweep.md` still
says "implementation pending"; `Map.groupBy` hands align-1 buffers to
generated code that does typed loads (works on supported targets, UB
by the letter of C, unlike the neighbouring properly-typed `reduce`
accumulator path); `Map.groupBy`'s callback-arity diagnostic cites Q24
where the surface is Q27; the generated reference's `string.slice`
summary no longer matches its behaviour after MAJOR 1.

**P18 is NOT COMPLETE**: MAJOR 2 is open.

## P18 review fixes (2026-07-26) — MAJOR 2 and MINOR 1–4 closed. P18 COMPLETE

**MAJOR 2 — stage 6, the `every` family on `FixedArray<T, N>`.** All
eight closure-taking members implemented at both arities. Return types
were derived from what a fixed-length in-place C array can promise
rather than copied from `T[]`: `map` returns **`U[]`** because the
element type may change, and `filter` returns **`T[]`** because the
result length is not known at compile time. Every member turned out
supportable, so the stage adds no rejection — the "leave it rejected
with an S014 saying why" fallback in the handoff was not needed.

Corpus `a68-q27-fixed-array-callbacks`, golden generated from node
v24.18.0 and **re-verified independently by the orchestrator** on a
hand-written equivalent — matched. No pre-existing `.expected` moved.

**MINOR 2** — `Map.groupBy`'s element and key slots are now `u64`-backed,
so the generated bridges' typed loads and stores always see 8-byte
alignment; a typed-access regression test guards it. The previous
`vec![0u8; …]` and `[0u8; 8]` were align-1 and worked only because
malloc and stack layout over-align on the supported targets.

**MINOR 3** — a wrong-arity `Map.groupBy` callback now cites Q27, its
actual surface, instead of Q24. Map/Set's own callbacks still cite Q24,
correctly.

**MINOR 4** — `string.slice` is table-driven through `StrFn` and its
generated summary now reads "using JS clamp and negative-index rules",
matching what MAJOR 1 recorded. The old summary's claim that "both
arguments are required" was **also wrong**: the checker confirms both
are optional, defaulting to `0` and to the end. The generated signature
is now `slice(start?: i32, end?: i32): string`.

**MINOR 1** — `js-api-sweep.md` updated: Q27 is implemented, and the
`FixedArray` row records the `U[]`/`T[]` return-type reasoning.

### P18 disposition

All CRITICAL (none), MAJOR and MINOR findings are closed. **P18 is
COMPLETE.**

Two items were opened by the review and are **not** P18 blockers; they
are carried forward:

1. **No benchmark exercises array callbacks.** `perf-gate`'s a22 and
   every `benchmarks/workloads/subscript/` entry avoid them, so stage
   5's and stage 6's per-element work is measured by nothing.
2. **The emitted-C figure needs an idle-machine re-measurement.**
   1.87x observed against 1.05x recorded for arm64; unattributable to
   P18, since a22 does not execute P18's code.

## Carried item 1 closed — the `callbacks` benchmark (2026-07-26)

The P18 review's coverage gap is closed: `benchmarks.md` Rev 1 adds a
ninth cross-language workload, `callbacks`, and it is implemented for
all six subjects. Q27's per-element branch in `call_value`/
`call_reduce` is now executed by a benchmark, so a regression there is
visible as a move in the `subscript-ship` row.

Parameters: N = 1 000 000, K = 20 rounds, seed `0x12345678`, the same
LCG `sort` uses. `filter` removes 250 000 elements per round, so the
pipeline is not a fixed-shape loop in disguise. Checksum
**−662567840**, identical across all six subjects — confirmed
independently by the orchestrator running node on the JS subject.

### First measurement (baseline, not a regression signal)

100 warm-up / 11 timed. Every subject within ±20% of its median.

| Subject | ×C | median |
|---|---:|---:|
| C | 1.00 | 13.072 ms |
| JSC | 5.38 | 70.280 ms |
| LuaJIT | 9.62 | 125.815 ms |
| **subscript-ship** | **20.84** | 272.433 ms |
| subscript-jit | 26.06 | 340.633 ms |
| V8 (Node.js) | 29.76 | 389.078 ms |

**How to read it, beyond what the contract already requires.** The C
subject writes loops over three buffers it allocates **once**;
subscript and the JS subjects call `map`/`filter`, each of which
returns a **fresh array every round** — 20 allocations of a million
elements per operation against C's one. So the ratio blends two
distinct costs, per-element call overhead and per-round allocation,
and attributing it to either without separating them would be wrong.
subscript-ship is faster than V8 here and slower than JSC and LuaJIT.

The implementer confirmed the generated ship C passes `indexed=1` to
the runtime for all three calls and that both tiers reach the
`arrops` helpers — the runtime is a static library taking `indexed` as
an external argument and the final link is without LTO, so the branch
under measurement is not specialized away. That check mattered: a
workload whose hot path the compiler had specialized out would have
measured nothing while appearing to pass.

### Run validity

Earlier attempts at 8/15, 20/11 and 30/11 were **discarded for spread
exceeding ±20%** — this machine has been compiling continuously all
session. At 100/11 the `callbacks` row is valid, but the runner still
invalidated three *other* C rows (`fib-loop`, `mandelbrot`, `primes`)
on OS-scheduling outliers and exited non-zero. Those rows are not
republished here; only `callbacks` is, and only because it passed.

### Carried item 2 remains open

The emitted-C figure still needs an idle-machine re-measurement
(1.87x observed against 1.05x recorded for arm64). This session cannot
supply it — the run above is direct evidence that the machine is not
quiet enough.

## P13 stage 1 — `JSON.stringify` (2026-07-26)

Implemented. `JSON.parse` is stage 2 and untouched.

Call-site monomorphized serializers, **no RTTI and no layout
descriptors** — §13.1's premise held under implementation, which was
the thing most worth checking, since the roadmap had named RTTI as
this phase's new machinery. One shared `sub_rt_json_*` runtime serves
both tiers.

Static cycle analysis emits two shapes as §13.2 requires: **no
tracking operations at all** for a type whose field graph cannot reach
a reference class from itself, and active-path tracking with a cycle
trap for one that can.

Corpus `a69-json-stringify`, golden generated from node v24.18.0 and
**re-verified independently by the orchestrator** on a hand-written
equivalent — matched (451 bytes). No pre-existing `.expected` moved.
Four pinned S014 rejections (`Map`, `Set`, `object`, function type)
and the `NaN`/`Infinity`/cycle traps.

`NaN` and `Infinity` appear as divergence witnesses in the generated
API reference. The P16 witness comparison is `Value(stdout) | Trap`
and a trap never agrees with a value, so trap-versus-`null` is
demonstrable — which is why §13.5 could require these as traps rather
than goldens.

### Contract correction: the escape set

§13.5 pre-registered "control characters as `\u00XX`". Measured on
node v24.18.0, that is wrong in two ways: five control characters take
**short escapes** (`\b \t \n \f \r` for U+0008/0009/000A/000C/000D),
and the rest of U+0000–001F take **lowercase** `\u00xx`. U+007F,
U+0080, U+2028, U+2029 and `/` all pass through unescaped. Recorded as
§13.2a.

This is the same failure the P18 review named five times: a
pre-registration asserting something no measurement had checked. The
contract now carries the measured set and says so.

## P13 stage 2 — `JSON.parse` (2026-07-26). P13 implementation complete

Implemented. `JsonResult<T>` is an ambient generic reference class on
Q24's machinery; the prelude declares it with a private constructor so
`JSON.parse<T>(text): JsonResult<T>` is `tsc`-clean. Parsing validates
the whole document before constructing any language value, so a
failure returns `ok = false` with no partial result and no trap —
which is what §13.4 required and the reason it was chosen over
trapping.

Corpus `a70-json-roundtrip`, `a71-json-parse`, `r60-json-parse-no-context`,
`r61-json-parse-date`. Node-comparable lines `cmp`-verified against
v24.18.0; the `ok`-flag lines are contract-derived, JS having no
`JsonResult`, no static `T` validation and no `unsafeDelete`. No
pre-existing `.expected` moved.

### Defect found in review — integer targets lost precision silently

`a71`'s first golden recorded `beyond-safe-i64=9007199254740992` for
the source `JSON.parse<i64>("9007199254740993")`. `runtime/src/json.rs`
had `number()` return `Option<f64>` and range-check *that* against the
target, so **every JSON number went through `f64` before the target
type was consulted** and exactness above 2^53 was gone first. A
different integer came back with `ok = true` and nothing reported it.

Fixed: an `i8`…`u64` target converts the number's **text** directly and
exactly, with `ok = false` if it is not an integer or does not fit.
`f32`/`f64` keep the `f64` path — `JSON.parse<f64>("9007199254740993")`
returning `…92` is correct, because `f64` cannot hold the value; the
`i64` case was not, because `i64` can.

Worth keeping as a distinction: **inexactness that belongs to the
target type is not a defect; inexactness the parser introduces before
the target is consulted is.**

### `Date` rejected as a `parse` target

The implementer reported that `Date` was an accepted target no JSON
node could ever match, an untagged ISO string being indistinguishable
from a `string` field of the same text. An unreachable-by-construction
target is the shape Q24 originally had with a literal `NaN` `Map` key.
Now S014, with the reason in the message; `Date` stays a `stringify`
output. Reject entry `r61`.

### Contract correction: what round-trips

§13.5 pre-registered a round-trip entry with `parse(stringify(x))`
equal to `x`. Too broad: `-0` cannot, because §13.3 has `stringify`
emit `0`, and `Date` cannot, because it is not a `parse` target.
Recorded as §13.4a. The corpus entry shows `-0` coming back as `0`
rather than omitting the case.

**Phase Review pending** before P13 is marked COMPLETE.
