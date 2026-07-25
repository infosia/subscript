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
  (`r50-parse-int-no-radix` is the §11.6-required entry — the corpus is
  right, the message undercounts); this tracking entry and the
  benchmark row were the outstanding §2/§11.6 items.

**Verified clean by the review** (~4.06 M values, both tiers): the Q14
correction is exactly node's notation everywhere, perturbs **zero**
in-range values, and is coherent for `f32` as well as `f64`; `toFixed`
matches node over 650 588 cases including the half-way rule
(`(1.005).toFixed(2)` → `"1.00"`), `digits` 0 and 100, `≥1e21`, `NaN`,
infinities and `f32` receivers; `parseFloat` matches over 80 094 cases
including every ECMA whitespace class; all six trap tuples are
identical across tiers; the whole §11.5 rejected surface is S014 with a
Q25 citation, and program-declared shadowing of `parseInt`/`isNaN`/
`Number` correctly wins over the intrinsic. No cross-tier divergence
anywhere.

## Gate (§11.6, all met — orchestrator-verified)

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
