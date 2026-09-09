# Four runtime differences from TypeScript, weighed

Date: 2026-09-09. A downstream round proposed three changes and one
documentation correction. This record holds the measurements, what
each difference costs to change, and which of them is an owner
decision.

Host: aarch64 macOS. subscript at `0f6a5c7`, dev JIT. node v24.18.0,
TypeScript 5.9.2. Every row below was measured twice, once by the
round and once here.

## The measurements

| Call | subscript | node |
|---|---|---|
| `"ab".padStart(4, "")` | trap `string-range` | `"ab"` |
| `"ab".padStart(1, "")` | `"ab"` | `"ab"` |
| `"ab".replaceAll("", "-")` | trap `string-range` | `"-a-b-"` |
| `"ab".split("")` | trap `string-range` | `["a", "b"]` |
| `"ab".charCodeAt(5)` | trap `string-range` | `NaN` |
| `"ab".repeat(-1)` | trap `string-range` | throws `RangeError` |
| `${-0.0}` as `f64` | `-0` | `0` |
| `"a-b".replaceAll("-", "[$&]")` | `a[-]b` | `a[-]b` |

## Every one is a decided divergence, and each carries a witness

`compiler/src/api_reference.rs` holds an executable divergence
witness for each: `q14-negative-zero`, `q21-pad-start-empty`,
`q21-pad-end-empty`, `q21-split-empty`, `q21-replace-all-empty`, and
`q21-char-code-oob`. A witness carries the subscript program, the
JavaScript program, and both outcomes, and the generated API
reference is gated. None of the four is an undocumented defect.

The round's report cited Q21 alone. It missed `stdlib.md` §8 and the
witnesses. §8 states the empty-pad rule **with its reason**:

> an empty `pad` with `len > length` traps (JS returns the string
> unchanged for empty pad — divergence recorded in Q21: silent
> non-padding hides bugs)

That reason stands on its own and matches design invariant 6, so the
trap is not an arbitrary hole. A caller who writes `padStart(4, "")`
asks for padding and receives none; the trap reports it.

## What each change costs

**1. Empty pad returns the receiver.** `specs/blocks/stdlib.md` §8;
`collisions.md` Q21; two witnesses in `compiler/src/api_reference.rs`;
`generated-docs/api-reference.md`; `runtime/src/ffi.rs` `str_pad` and
its unit test `ffi_str_pad_truncation_no_op_copy_and_empty_pad_trap`;
one new accept corpus entry. No existing golden moves, because no
corpus program pads with an empty fill.

**2. Negative zero formats as `0`.** `collisions.md` Q14; the
`q14-negative-zero` witness; `runtime/src/fmt.rs` at two sites plus
its unit tests; three goldens move: `a40-math.expected`,
`a45-array-fn.expected`, `a49-f16-conversions.expected`. Q14 already
survived one correction, and the test it applied there was whether a
divergence had been recorded. One is recorded here, so this is a
decision, not a consequence.

**3. Empty separator and empty pattern.** `collisions.md` Q21;
`stdlib.md` §8; two witnesses; `runtime/src/ffi.rs`;
`runtime/src/strops.rs`; new corpus for the boundary policy; and a
new collision entry, because the result diverges from Q5's byte rule
and from JavaScript's UTF-16 units at the same time. `split("")` on a
byte-measured string cannot return bytes without producing invalid
UTF-8, so the policy must be code points, which is a third answer
that neither existing rule gives.

## Recommendation, corrected 2026-09-09

The first version of this record weighed each proposal by what it
costs in contract text and moved goldens, and recommended that none
proceed. **The owner corrected the test: the point is to match
TypeScript, not to keep the records still.** Being recorded makes a
divergence decided. It does not make it right.

Re-measured under that test, all three proceed. §95 holds the rules.

**Each is an exception inside a rule that otherwise follows ECMA.**

- Empty pad: `"ab".padStart(1, "")` returns `"ab"` and reports
  nothing, and `"abcd".padStart(2, "x")` does too. The trap fires only
  above the receiver length, so it reports an arbitrary subset of the
  mistake `stdlib.md` §8 names.
- Negative zero: Q14 names ECMA's `Number::toString` as its reference
  and follows it for the exponent thresholds, `NaN` and `Infinity`.
  `toFixed` follows it too, measured: `(-0.0).toFixed(2)` is `0.00`
  here and under node. Interpolation is the one exception, with no
  stated reason.
- Empty separator: this record's first version claimed the change
  needs a new Unicode boundary policy. **That was wrong.** `slice`
  already traps on an offset that is not on a UTF-8 boundary, so the
  rule exists: a produced piece starts and ends on a boundary.
  Splitting at every boundary applies it at every position. No new
  collision entry is needed.

**The non-ASCII result is better than JavaScript's, not worse.**
Measured: `"😀a".split("")` is `["\ud83d", "\ude00", "a"]` under
node, two lone surrogates. This language returns `["😀", "a"]`, which
is valid UTF-8, under Q5's already-recorded divergence.

**Two traps stay.** `charCodeAt` out of range keeps its trap, because
the return type carries no NaN and the alternative is a silent wrong
integer. `repeat(-1)` keeps its trap, because JS throws there too, so
both implementations reject it.

**Cost.** Four goldens move; the landing section below carries their
before and after bytes. Five divergence witnesses retire from
`compiler/src/api_reference.rs` and one is added for the
supplementary-character split. *(Corrected 2026-09-09: this line first
named three and counted `corpus/` alone.)*

The documentation correction was the one plain defect, and it landed
at `0f6a5c7`.

## Red at the contract pin

Measured at `10c2db7`, aarch64 macOS, dev JIT, `subscript run`:

| Program | exit | first line |
|---|---:|---|
| `"ab".padStart(4, "")` | 1 | `trap [string-range]: padStart(4): an empty pad cannot reach the target length (string length 2)` |
| `"ab".padEnd(4, "")` | 1 | `trap [string-range]: padEnd(4): an empty pad cannot reach the target length (string length 2)` |
| `"ab".split("").length` | 1 | `trap [string-range]: split(""): an empty separator is not accepted` |
| `"ab".replaceAll("", "-")` | 1 | `trap [string-range]: replaceAll("", ...): an empty pattern is not accepted` |
| `${x}` with `const x: f64 = -0.0` | 0 | `-0` |
| `${x}` with `const x: f32 = -0.0` | 0 | `-0` |

The node results for the same six are `"ab"`, `"ab"`, `2`, `"-a-b-"`,
`0`, `0`, measured on node v24.18.0.

## Landed, 2026-09-09

§95 landed at `9f1b894`. Final:
`gate quick 9f1b894 clean debug 1348/0/2 skips 2 goldens-moved 0 exit 0`,
hygiene 0. The implementation round's full gate:

```
gate full b8498614c3b6c3120ccdda9711b111068ef4cc6d dirty:27 debug 1348/0/2 release 1346/0/2 skips 2/0 clippy 7/18/13 goldens-moved 3 exit 0
```

`goldens-moved 3` counts `corpus/` alone; the fourth is
`examples/e07-determinism.expected`.

### Every ASCII shape matches node

Measured here after the change, both sides:

| Row | subscript | node |
|---|---|---|
| `padStart(4,"")` / `padEnd(4,"")` / `padStart(1,"")` | `ab ab ab` | same |
| `"".split("")` / `"a"` / `"ab"` | `0 1 a,b` | same |
| `"".replaceAll("","-")` / `"a"` / `"ab"` | `- -a- -a-b-` | same |
| `"ab".replace("","-")` | `-ab` | same |
| `${-0.0}` at f64, f32, and `toFixed(2)` | `0 0 0.00` | same |
| `"aé".split("")` | `a,é` | same |
| `"😀a".split("")` | `😀,a` | two lone surrogates, then `a` |
| `"😀".replaceAll("","-")` | `-😀-` | `-` around each surrogate |

### Two corpus entries joined the node comparison set

`a40-math` and `a45-array-fn` were `js-comparable: no Q14`. Both are
now `js-comparable: yes`. `a49-f16-conversions` drops Q14 and keeps
C3 and Q23.

### The reason that kept the `-0` spelling, and why it failed

Q25 and Q28 both stated it: `${x}` is the only general-purpose
number-to-string path, so losing the sign there discards information
the program cannot otherwise see. Measured, the program has three
other ways:

| Expression | `-0.0` | `0.0` |
|---|---|---|
| `1.0 / x` | `-Infinity` | `Infinity` |
| `Math.f32ToBits(x as f64)` | `2147483648` | `0` |
| `Math.atan2(x, -1.0)` | `-3.141592653589793` | `3.141592653589793` |

The first two are how ECMAScript programs read it too.

### Two contract errors this round found, both mine

The section named three moved goldens and said no other moves. It
counted `corpus/` alone, and `examples/` carries goldens too. The
section also said Q14 states no reason for the spelling; Q14 does
not, but Q25 and Q28 do, and the reason had to be measured rather
than assumed absent.

### The four moved goldens, before and after

Before is `4dcbd98`, the commit that precedes §95. After is the
committed file. Sizes are bytes; digests are SHA-256 of the whole
file.

| File | Before | After | Changed lines |
|---|---|---|---|
| `a40-math.expected` | 621 bytes, `512088e51977c4a1` | 618 bytes, `8be7748a2c71373b` | 42, 43, 48 |
| `a45-array-fn.expected` | 186 bytes, `b77c418fe154fb46` | 185 bytes, `41ec99fe636ce00e` | 1 |
| `a49-f16-conversions.expected` | 113 bytes, `16e691d7b8845957` | 112 bytes, `611d506cdd9a914d` | 6 |
| `e07-determinism.expected` | 152 bytes, `66fabb832dd9fdc9` | 151 bytes, `cf88317cd234eaba` | 6 |

`corpus/accept/a40-math.expected`

- line 42: `round(-0.4) -0` becomes `round(-0.4) 0`
- line 43: `sign(-0) -0` becomes `sign(-0) 0`
- line 48: `min(0,-0) -0` becomes `min(0,-0) 0`

`corpus/accept/a45-array-fn.expected`

- line 1: `map <0.5> <2> <-0>` becomes `map <0.5> <2> <0>`

`corpus/accept/a49-f16-conversions.expected`

- line 6: `negative-zero -0` becomes `negative-zero 0`

`examples/e07-determinism.expected`

- line 6: `number=1234.5678|1234.57|-0` becomes `number=1234.5678|1234.57|0`

