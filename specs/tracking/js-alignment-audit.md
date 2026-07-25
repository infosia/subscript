# JS-alignment audit against Boa (2026-07-25)

Standing question: for every place this language gives up on matching
JS — a recorded divergence, or an API rejected as impractical — **is
there a solution we did not look for?** The reference point is
[Boa](https://github.com/boa-dev/boa), a JS engine written in Rust and
verified against test262: it faces the same problems in the same
language, so "Boa does not solve it either" is evidence, and "Boa
solves it with X" is a solution we can evaluate.

Owner instruction, 2026-07-25, prompted by a concrete failure recorded
below. The rule it produced: **a negative claim — "no solution exists",
"this would need a hand-rolled X" — needs investigation most of all.**
The project already required running another system before claiming its
behaviour; that discipline was applied to `node` but not to the claim
that nothing could match it.

## Closed by the audit

### Q14 tie-breaking — was recorded as unfixable, is fixed

The P12 review measured 339 divergences per 3 010 916 doubles: Rust's
shortest-round-trip writer breaks an exact decimal tie away from zero,
ECMA breaks to even. Recorded as a divergence with the reasoning that
matching ECMA "needs a custom shortest-float writer with tie-to-even;
a hand-rolled one is a worse risk than the 0.011 % it would close".

Boa uses **`ryu-js`** — Ryū forked for ECMA semantics — which was
already in the local cargo cache. Verified before adopting: 200 000
random `f64` bit patterns, `ryu_js::Buffer::format` versus `String(v)`
in node, **zero divergences**. Adopted (`=1.0.3`, runtime only, still
behind the opaque `sub_rt_*` symbols per `stdlib.md` §0.2).

It closed more than the tie: the `[1e-6, 1e21)` **exponent thresholds**
that P12 hand-wrote, and the hand-written **`toFixed`** rounding
(`format_to_fixed`, the same call Boa makes), both became crate
behaviour. Net **−111 lines of hand-written float code**. Post-change:
200 000 patterns through the language, zero divergences from node on
either tier; no golden moved; no benchmark row moved. The Q14 `-0`
spelling is kept as the one deliberate divergence.

### Q21 ASCII-only case folding — was an unnecessary limit

`toUpperCase`/`toLowerCase` were limited to ASCII on the reasoning that
full case folding needs Unicode tables. It does — and **Rust's standard
library already has them**. Boa's non-locale path calls
`str::to_uppercase()`/`to_lowercase()` (wrapped by `cow-utils` only to
avoid allocating when unchanged); ICU enters only for the `toLocale*`
variants, behind Boa's `intl` feature.

Verified: 15 hard cases against node — `ß`→`SS`, `ﬄ`→`FFL`, `ﬁ`→`FI`,
final sigma `ΣΣς`→`ΣΣΣ`/`σσς`, `ᾀ`→`ἈΙ`, `µ`→`Μ`, `İ`, `ı`, `ǆ`, `ʼn`
— **15/15 identical**, special casing included.

**Lifted** (commit `376b5ee`): both tiers now use the Rust std path,
`trim` uses the explicit ECMA WhiteSpace + LineTerminator predicate
rather than Rust's `trim` (which removes `U+0085` where ECMA does not,
and skips `U+FEFF` where ECMA does not), and `a60-string-unicode`
matches node line-for-line. Byte-length growth is handled — a lone
`İ` (U+0130) lowercases from 2 UTF-8 bytes to 3. `length`/`slice`/
`charCodeAt` keep their byte semantics (Q5), which is a separate
question this did not touch.

## Confirmed as genuine — Boa needs the same thing we lack

- **Local-time `Date` accessors, `getTimezoneOffset`, `toLocale*`.**
  Boa does not carry a tz database: `local_time` delegates to
  `HostHooks::local_timezone_offset_seconds`. That is the same shape as
  this language's invariant 4 — platform capabilities come from the
  host across the C ABI. The divergence is legitimate, and the route to
  supporting it later is the host C facade, not an engine change.
- **`localeCompare`, `Intl`, locale-sensitive case.** Boa needs ICU
  (`icu_normalizer`, its case mapper) behind a feature. Consistent with
  the stdlib's locale non-goal.

## Confirmed as correct language design — not give-ups

Q26's rule (implement a JS API that exists and is affordable, whatever
the demand) does not reach these, and the reasons are **three
different ones**. Keeping them apart is what lets a future revisit
reopen the weak class without reopening the strong one.

**Implementing it would introduce a defect.** `Number(x)` and the
global `isNaN`/`isFinite` coerce; no-argument `sort` string-coerces the
elements; `reduce` without `init` changes meaning with arity; `find`
has no miss value for a scalar element type. Each would compile and
then answer wrongly in silence. Boa has full JS semantics and never
faces the problem; these follow from this language's type system, not
from difficulty. **This class is the strongest, and cost is not the
objection.**

**Implementing it costs nothing and buys nothing.** `Math.imul` is
`a * b` on `i32`; `fround` is `x as f32`; `substring`/`substr`/`at`/
`charAt` restate `slice` and `charCodeAt`. Boa implements `imul` and
`fround` because JS numbers are `f64` and there is no other way to ask
for an `i32` multiply or an `f32` round — this language has both
directly, so rejecting them removes a workaround rather than a
capability. Nothing breaks if they are added; there is simply a second
spelling. **This class is the weak one** — it is held on judgement, not
on a defect, and Q26's rule applied strictly would take it. Contrast
`toString(radix)`, which was accepted precisely because its absence
*did* cost a capability: hexadecimal could be read and not written.

**The machinery does not exist.** The iterator protocol —
`keys`/`values`/`entries`/`for…of`/spread — is not a rejected API but
an absent language feature. Neither of the above reasons applies.

**Determinism divergences** (seeded `Math.random`, `Date` trapping
instead of Invalid-Date) — deliberate, and Boa's unseeded behaviour is
what we are avoiding.

## Decided after the audit

### `includes` / `Map` keys and `NaN` — adopted SameValueZero

Was: one equality rule (`===`) everywhere, so `NaN` was never found.
Boa implements both rules; the give-up here was never difficulty, it was
a preference for a single equality story. Measured against node
v24.18.0:

| expression | node | this language, before |
|---|---|---|
| `[NaN].indexOf(NaN)` | `-1` | `-1` (already agreed) |
| `[NaN].lastIndexOf(NaN)` | `-1` | `-1` (already agreed) |
| `[NaN].includes(NaN)` | `true` | `false` |
| `map.get(NaN)` after `set(NaN, v)` | `v` | miss |
| `set.has(NaN)` after `add(NaN)` | `true` | `false` |
| `-0` key, then `keys()` | `[0]` | `-0` stored |
| two `NaN`s with different payloads | one entry | — |

So the divergence was two methods wide, not language-wide.
**Decision (owner, 2026-07-25): adopt.** Q22 `includes` and Q24 float
keys move to SameValueZero; `indexOf`/`lastIndexOf` stay on `===`
because that is what JS uses there. The cost is stated in Q22: JS's own
`indexOf`/`includes` inconsistency is imported. It was accepted because
the old rule produced a silently wrong `includes` answer and made a
`NaN` `Map` key insertable but unreadable, and because it *removes* a
rule — Q24's compile-time rejection of a literal `NaN` key existed only
to hide the unreachable entry.

Boa's own implementation is `same_value_zero` in
`core/engine/src/value/equality.rs`. Note that Boa's `RationalHashable`
(`core/engine/src/value/hash.rs`) compares with `Number::same_value`
but hashes `self.0.to_bits()` — equal-but-differently-hashed values are
possible in principle, so **this language does not copy that shape**.
It normalizes instead: `-0` becomes `+0` at insert, and the hash
already folds both zeros to the same bits, so plain bit hashing stays
consistent with the equality rule.

### `clz32`, `toString(radix)`, `toExponential`, `toPrecision` — all accepted (Q26)

The audit flagged these because their recorded rejection reasons did
not survive reading: `clz32` was rejected as a "JS-number op" alongside
`imul` and `fround`, but unlike those two it has **no replacement
spelling in this language**; and the three `Number` methods were
rejected as "not in v1", a scope statement with no cost behind it.

Measured cost, from Boa's
`core/engine/src/builtins/number/mod.rs` (934 lines total):

| API | lines | dependency |
|---|---:|---|
| `to_exponential` | ~100, plus ~90 in the shared `flt_str_to_exp` / `round_to_precision` helpers | none |
| `to_precision` | ~125, sharing those helpers | none |
| `to_js_string_radix` | ~120 | none |
| `clz32` | 1 (`leading_zeros()`) | none |

About 440 lines of pure computation with no external dependency. Note
`ryu-js` does **not** supply these — it exposes only `format`,
`format_finite` and `format_to_fixed`, so this is code we write.

**Decision (owner, 2026-07-25): implement all four**, under the
standing rule that a JS API which exists and is implementable at
realistic cost is implemented regardless of expected demand. Q26
records the contract; Q19 and Q25 were amended.

`imul` and `fround` stay rejected as duplicate spellings (see the
previous section for the classification). `clz32` differs from them
precisely because nothing in the language counts leading zeros.

Two node v24.18.0 measurements became normative traps, both cases where
the C tier's obvious lowering is wrong:

- `Math.clz32(0)` is `32`; C's `__builtin_clz(0)` is undefined.
- `(0).toExponential(2)` is `0.00e+0`; C's `%e` gives `0.00e+00`.

## Open — re-examine, not yet decided

Nothing open. Every divergence and every rejection recorded at the time
of the audit has been either closed, confirmed as a genuine missing
prerequisite, confirmed as language design that costs no capability, or
decided by the owner.

## How to keep this current

Any new recorded divergence, or any API rejected on grounds of
difficulty, gets a line here saying what Boa does. A divergence with no
entry has not been audited.
