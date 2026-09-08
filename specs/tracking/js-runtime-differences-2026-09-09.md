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

**Cost.** Three goldens move: `a40-math.expected`,
`a45-array-fn.expected`, `a49-f16-conversions.expected`. Five
divergence witnesses retire from `compiler/src/api_reference.rs` and
one is added for the supplementary-character split.

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
