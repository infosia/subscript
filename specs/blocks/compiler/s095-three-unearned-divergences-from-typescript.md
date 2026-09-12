<!-- §95 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 95. Three unearned divergences from TypeScript

*(Owner decision 2026-09-09.)* Origin: a downstream report measured
four runtime differences from TypeScript. The first review of it
weighed what each change costs in contract text and moved goldens.
That is the wrong test. The test is whether the divergence earns its
keep.

Each of the three below is an exception inside a rule that otherwise
follows ECMA, and each one contradicts this language's own behaviour
one step away. Being recorded makes a divergence decided. It does not
make it right.

Measured at `0f6a5c7` on aarch64 macOS, dev JIT, against node
v24.18.0:

| Call | subscript | node |
|---|---|---|
| `"ab".padStart(4, "")` | trap `string-range` | `"ab"` |
| `"ab".padStart(1, "")` | `"ab"` | `"ab"` |
| `${-0.0}` as `f64` | `-0` | `0` |
| `(-0.0).toFixed(2)` | `0.00` | `0.00` |
| `"ab".split("")` | trap `string-range` | `["a", "b"]` |
| `"ab".replaceAll("", "-")` | trap `string-range` | `"-a-b-"` |

### 95.1 An empty pad returns the receiver

1. `padStart(len, "")` and `padEnd(len, "")` return the receiver
   unchanged, at every `len`. The trap retires.

The evidence is the language's own inconsistency. `"ab".padStart(1,
"")` returns `"ab"` and reports nothing. `"abcd".padStart(2, "x")`
returns `"abcd"` and reports nothing. The trap fires only when `len`
is above the receiver length, so it reports one arbitrary subset of
the mistake that `stdlib.md` §8 names. The recorded reason, "silent
non-padding hides bugs", does not separate those cases.

ECMA's StringPad returns the receiver when the fill string is empty.

### 95.2 Negative zero interpolates as `0`

2. Template interpolation of a negative zero produces `0`, for `f32`
   and `f64`. The value keeps its sign; only the spelling changes.

Q14 names ECMA's `Number::toString` as its reference and follows it
for the exponent thresholds, for `NaN`, and for `Infinity`. `toFixed`
follows it too: `(-0.0).toFixed(2)` is `0.00` here and under node.
Interpolation is the one exception.

*(Corrected 2026-09-09. The section first said Q14 states no reason.
Q14 does not, but Q25 and Q28 both do, in the same words: `${…}` is
the only general-purpose number-to-string path, so losing the sign
there discards information the program cannot otherwise see. That
reason does not survive measurement.)* A program reads the sign of a
zero three other ways, measured on this host:

| Expression | `-0.0` | `0.0` |
|---|---|---|
| `1.0 / x` | `-Infinity` | `Infinity` |
| `Math.f32ToBits(x as f64)` | `2147483648` | `0` |
| `Math.atan2(x, -1.0)` | `-3.141592653589793` | `3.141592653589793` |

The first two are how ECMAScript programs read it as well. The sign
is not lost, so the reason that kept the spelling is void.

Three goldens move under the §2 procedure: `a40-math.expected`,
`a45-array-fn.expected`, `a49-f16-conversions.expected`.

### 95.3 An empty separator splits at every boundary

3. `split("")` returns one piece for each UTF-8 code point.
   `"".split("")` is `[]`.
4. `replaceAll("", r)` inserts `r` at every code-point boundary,
   including the start and the end. `"".replaceAll("", r)` is `r`.
   `replace("", r)` inserts `r` once at the start, which is ECMA's
   first-occurrence rule.
5. This adds no boundary policy. `slice` already traps on an offset
   that is not on a UTF-8 boundary, so every produced piece must start
   and end on one. Splitting at every boundary is that same rule,
   applied at every position.
6. ASCII matches JS exactly. Non-ASCII carries Q5's recorded
   byte-against-UTF-16 divergence and nothing else. Measured:
   `"😀a".split("")` is `["😀", "a"]` here and
   `["\ud83d", "\ude00", "a"]` under node, which is two lone
   surrogates. This language produces valid UTF-8 and JS does not. No
   new collision entry is required, and the divergence witness for the
   supplementary-character shape replaces the two that retire.

### 95.4 What does not change

`"ab".charCodeAt(5)` keeps its trap. JS returns `NaN`, and the return
type carries no NaN, so the alternative is a silent wrong integer.

`"ab".repeat(-1)` keeps its trap. JS throws a `RangeError`, so both
implementations reject the call. That is no divergence in kind.

Q5's byte-measured `length` and `slice` keep their divergence. It is
the representation choice, and §95.3 rule 6 depends on it.

### 95.5 Sites

- `runtime/src/ffi.rs`: `str_pad` drops its empty-pad trap and returns
  the receiver length; the `split` and `replaceAll` entry points drop
  their empty-argument traps.
- `runtime/src/strops.rs`: the split and replace algorithms gain the
  empty-pattern case at code-point boundaries.
- `runtime/src/fmt.rs`: the two negative-zero sites return `0`.
- `specs/blocks/stdlib.md` §8: the `padStart`/`padEnd`, `split`,
  `replace` and `replaceAll` entries state the new rules.
- `specs/blocks/collisions.md`: Q21 and Q14 record what is left.
- `compiler/src/api_reference.rs`: `q21-pad-start-empty`,
  `q21-pad-end-empty`, `q21-split-empty`, `q21-replace-all-empty` and
  `q14-negative-zero` retire. One witness is added for the
  supplementary-character split. `generated-docs/` regenerates.

### 95.6 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the six measured rows above, recorded
on this host with their exit codes.

1. One accept entry per rule area, each `js-comparable: yes` where the
   shape is ASCII: an empty pad at three targets; an empty separator
   and an empty pattern on an ASCII receiver, an empty receiver, and a
   one-character receiver; a negative zero interpolated at `f32` and
   `f64`.
2. One accept entry for the non-ASCII shapes, `js-comparable: no`
   citing Q5: `"aé"` and `"😀a"` under `split("")` and
   `replaceAll("", "-")`. Its header states the `node` result beside
   this language's.
3. Four goldens move, and each carries its before and after bytes in
   the tracking note under the §2 procedure:
   `corpus/accept/a40-math.expected`,
   `corpus/accept/a45-array-fn.expected`,
   `corpus/accept/a49-f16-conversions.expected`, and
   `examples/e07-determinism.expected`. *(Corrected 2026-09-09. The
   section first named three and said no other golden moves. It
   counted `corpus/` alone, and `examples/` carries goldens too.)*
   `examples/e07-determinism.ts` also quotes its own output in a
   comment, which the change makes wrong; correct that comment with
   it. `corpus/accept/a59-number-to-fixed.expected` does not move:
   its `-0.00` comes from `(-0.0001).toFixed(2)`, a negative value,
   and node prints `-0.00` there too.
4. Unit tests in the same commit: `str_pad` with an empty fill at a
   target below, equal to, and above the receiver length; the split
   and replace algorithms at a code-point boundary and at the string
   ends; `fmt_f32` and `fmt_f64` on both zeros. Every negative test
   carries a positive control.
5. The `node` comparison runs for every `js-comparable: yes` entry.
6. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every golden
   byte-identical except the four that item 3 names.
