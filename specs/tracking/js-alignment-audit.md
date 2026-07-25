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
— **15/15 identical**, special casing included. **Follow-up: lift the
ASCII restriction.** Not yet done.

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

- **`Math.imul` / `fround`.** Boa implements both because JS numbers are
  `f64` and there is no other way to ask for an `i32` multiply or an
  `f32` round. This language has `i32 * i32` and `x as f32` directly,
  so the rejection removes a workaround rather than a capability.
- **Sound-typing rejections** (`find` with no miss value, no-init
  `reduce`, no-arg `sort`, the iterator protocol, `Number(x)` coercion)
  — Boa has full JS semantics and never faces the problem; these follow
  from this language's type system, not from difficulty.
- **Determinism divergences** (seeded `Math.random`, `Date` trapping
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

## Open — re-examine, not yet decided

- **`Math.clz32`.** Rejected as a "JS-number op", but unlike `imul` and
  `fround` it has **no replacement in this language** — there is no
  count-leading-zeros primitive. The rejection reason does not hold for
  it. Candidate for addition as a bit-manipulation primitive rather
  than as a JS compatibility shim.
- **`Number.prototype.toPrecision` / `toExponential`, `toString(radix)`.**
  Rejected in Q25 without a difficulty claim. Boa implements all three
  (`to_js_string_radix` is ~40 lines). If they are wanted, nothing
  blocks them.

## How to keep this current

Any new recorded divergence, or any API rejected on grounds of
difficulty, gets a line here saying what Boa does. A divergence with no
entry has not been audited.
