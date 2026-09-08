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

## Recommendation

Do not take any of the three without an owner decision. Each one
changes a recorded divergence that carries a rationale and an
executable witness. Proposal 3 also needs a new collision entry
before it can be implemented, because it invents a boundary policy.

The documentation correction was a defect and it is fixed at
`0f6a5c7`. Q21 said the trapping cases are ones where "JS returns NaN
or silent no-ops", which holds for one of the five, and it said
`replace` and `replaceAll` do not interpret `$` substitutions, which
Q27 reinstated on 2026-07-25 and the runtime implements.

If the owner takes proposal 1, it is the cheapest of the three: no
golden moves and no new policy. Proposal 2 is next. Proposal 3 needs
a contract section first.
