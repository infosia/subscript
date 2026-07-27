# JS API sweep — what is implemented, what is deferred, and why

Companion to `js-alignment-audit.md`. The audit asked whether each
*divergence* had a solution we had not looked for; this file asks the
same of each *rejection*, and records the standing answer so a later
session does not re-derive it.

## The rule (owner, 2026-07-25)

**A JS API that exists and is implementable at realistic cost is
implemented, regardless of expected demand.** Two clarifications the
owner gave when the rule was applied:

- **Low demand is not a reason to reject.** "Not in v1" is a scope
  statement, not a cost. Where cost was the real content, say the cost.
- **Being a second spelling of an existing operation is not a reason to
  reject.** An API that duplicates `slice` or `a * b` is still
  implemented if JS has it.

Only two reasons survive the rule, and they are recorded per API below:
implementing it would **introduce a defect**, or the project **lacks a
prerequisite** it cannot cheaply acquire.

## Accepted — implemented

| API | contract | status |
|---|---|---|
| `Number.prototype.toString(radix)` | Q26, `stdlib.md` §11.5 | `86f925a`, corpus `a62` |
| `Number.prototype.toExponential` | Q26, §11.5 | `86f925a`, `a62` |
| `Number.prototype.toPrecision` | Q26, §11.5 | `86f925a`, `a62` |
| `Math.clz32` | Q26, `stdlib.md` §1 | `86f925a`, `a62` |

Both normative traps held. Radix 10 **delegates to the existing Q14
formatter** rather than reimplementing it, so the two agree by
construction and `a62` prints them side by side. `clz32` goes through
`sub_rt_math_clz32` over `u32::leading_zeros`, and a `cemit` test
asserts the emitted C does **not** contain `__builtin_clz`, which is
undefined at zero where ECMA defines `clz32(0)` as `32`. Exponents are
unpadded (`(0).toExponential(2)` is `0.00e+0`, not C's `0.00e+00`).
`a62`'s golden was regenerated from node v24.18.0 and `cmp`'d
independently of the implementer. `r48`/`r49` were repurposed from the
old contract's `toPrecision`/`toString(16)` rejections to the
required-argument S014s.

## Accepted by the rule — contracted as Q27, fully implemented

The sweep of 2026-07-25 found these fail no surviving reason: each
exists in JS, introduces no defect, and needs no prerequisite the
project lacks. Contracted as **Q27** (`f51d480`) and **fully implemented
2026-07-26** across six stages (`stdlib.md` §12), corpus `a63`–`a68`.
The sixth stage — the `every` family on `FixedArray` — was added after
the P18 Phase Review found that group contracted but never staged.

**Writing the contract corrected the table in three places, each found
by measuring rather than reasoning** — the entries below are the
corrected form:

- **`shift` returns `undefined` on an empty array in JS**, which looked
  like the miss-value problem that keeps `find` and `at` out. It is
  not: `pop` already **traps** when empty (Q4/Q15), so the same rule
  covers `shift` and no sentinel is needed.
- **`splice` and `unshift` are variadic in JS** (`splice(1, 2, 9, 9,
  9)`, `unshift(a, b, c)`). The language has no variadic parameters —
  the same missing prerequisite that keeps `Math.max` at two
  arguments — so the accepted forms are delete-only `splice` and
  single-element `unshift`. **A recorded subset, not parity**, and the
  contract requires a reject entry naming the reason so a reader can
  tell the two apart.
- **The callback `array` parameter is not in the same class as the
  index parameter.** `f(v, i)` passes a value and an integer; `f(v, i,
  arr)` passes a reference to the container being iterated, which is
  the defect the P15 review found in aggregate `Map.forEach` and
  contradicts C5. Index accepted, array rejected.

| area | API | note |
|---|---|---|
| `Math` | `imul`, `fround` | duplicate spellings of `a * b` on `i32` and `x as f32`; the rule reaches them regardless |
| `String` | `substring`, `substr` | **not** duplicates of `slice`: measured on node v24.18.0, `"hello".substring(-2,3)` is `"hel"` (negatives clamp to 0, arguments swap when reversed) where `slice(-2,3)` is `""` |
| `String` | `charAt` | total — out of range is `""`, not `undefined`, so no miss value is needed |
| `String` | `concat` | duplicate of `+` |
| `String` | `codePointAt` | out of range should trap, as `charCodeAt` already does |
| `String` | the position argument of `startsWith`/`endsWith` | currently rejected as "optional arguments not accepted" |
| `String` | `$$`/`$&` substitution in `replace`/`replaceAll` | needs no regex engine; currently a recorded Q21 divergence |
| `Array` | `reduceRight` with a required `init` | passes the same arity rule that `reduce` passes |
| `Array` | `splice` (delete-only), `shift` (traps when empty, as `pop` does), `unshift` (one element, as `push`), `copyWithin` | JS makes `splice`/`unshift` variadic; the language has no variadic parameters, so these are a recorded subset |
| `Array` | **the index parameter on callbacks** | `map((v, i) => …)`; the largest item here by real-world use, and the one that touches the checker's arity machinery. The third `array` parameter stays **rejected** — see below |
| `Array` | the `every` family on `FixedArray` | was a "v1 is `T[]` only" scope rejection. Implemented as §12 stage 6; `map` returns `U[]` and `filter` returns `T[]`, since a fixed-length receiver cannot give a fixed-length result when the element type or the length changes |
| `Map`/`Set` | `groupBy`, ES2024 set algebra (`union`, `intersection`, `difference`, `symmetricDifference`, `isSubsetOf`, `isSupersetOf`, `isDisjointFrom`) | |
| `Number` | `Number.parseInt`, `Number.parseFloat` | verified `=== parseInt` / `=== parseFloat` on node — pure aliases |

## Rejected — implementing it introduces a defect

The strongest class. Cost is not the objection and the rule does not
reach these.

- **Coercion.** `Number(x)`, the global `isNaN`/`isFinite`, `String` as
  a value or constructor. Coercion is the unsoundness the language
  exists to reject; adding these imports it.
- **Arity that changes meaning.** `sort` with no comparator
  (string-coerces the elements), `reduce` with no `init`. Q22's rule,
  and the reason Q25 requires `parseInt`'s radix and Q26 requires
  `toString`'s.
- **No miss value.** `find`/`findLast`, `at` (both `String` and
  `Array`), and `Map.get` where `V` is scalar. JS returns `undefined`;
  this language has no `undefined`, and `T | null` does not cover
  scalars — `string | null` is itself rejected (S011: unions are
  limited to `Ref | null`, and `string` is not a reference shape).
  `findIndex`, `charAt` and `getOr` are the total spellings.
  **`at` was misclassified as a duplicate spelling during the sweep and
  corrected**: it is this class, not a redundancy.
- **Mutation of an immutable value.** `Date` setters. A `Date` erases
  to `i64` and is a value; a setter would break that.

## Rejected — a prerequisite the project does not have

Not a cost question. Each would be decided by acquiring the
prerequisite, which is a separate decision from this rule.

| missing | APIs |
|---|---|
| locale data | every `toLocale*`, `localeCompare` |
| timezone database | `Date` local-time accessors, `getTimezoneOffset`, `Date.parse`, the `toString` family, the multi-argument `Date` constructor, `Date` in a template literal |
| Unicode normalization tables | `normalize` (Boa needs `icu_normalizer` for the same reason) |
| **a regular-expression engine** | `match`, `matchAll`, `search`, the regex forms of `split` and `replace` |
| **an iterator protocol** | `keys`/`values`/`entries`, `for…of`, spread, construction from an iterable |
| **tagged template machinery** | `String.raw` |
| **variadic parameters** | `Math.max`/`min`/`hypot` beyond two arguments |

The last four are **language features, not library gaps**. Adding any
of them is a phase of its own and is outside this rule; recorded here
so that "why is `for…of` rejected" has one answer.

### Wanted: a regex engine and an iterator protocol

**Owner, 2026-07-25: both are wanted, at high priority, to be designed
later.** Not scheduled; recorded here with what the sweep already
found, so the later design does not start from zero.

**Iterator protocol — most of the machinery is already built.** The
standing note that "the language has no iterator protocol" (Q24) is
imprecise. The language has generator functions and the iterator
*result shape* today: `corpus/accept/a20-coroutine-generator.ts`
declares `function* sequence(limit: i32)` with `yield`, and drives it
with `generator.next()`, reading `step.done` and `step.value`. The C
tier lowers generators through CPS (`compiler.md` §11 coverage list).
So suspendable functions, the `{done, value}` step shape, and both
tiers' lowering of them exist. What is missing is the **binding**:

- a way to say "this type is iterable" (JS uses `Symbol.iterator`;
  symbols are not in the language, so this needs its own spelling)
- `for…of` desugaring onto that binding
- `keys`/`values`/`entries` on `Array`/`Map`/`Set` returning iterators
- spread, and construction from an iterable (`new Map([[k, v]])`)

Two constraints the design must answer, both already decided
elsewhere: iteration order for `Map`/`Set` is **normative** insertion
order (Q24), so the protocol inherits a fixed order rather than
choosing one; and callbacks today are **non-escaping by construction**
(C5), whereas an iterator is a stateful object that outlives the call
that made it — which is a memory-model question (invariant 2, no
implicit GC), not a syntax question.

**Regex — Boa's engine is a reusable Rust crate.** Boa does not
hand-roll one: `core/engine/Cargo.toml` depends on **`regress`**
(pinned `0.10.4` with the `utf16` feature in Boa's workspace root), a
regex engine written for JS semantics rather than Rust's `regex`
crate — which matters, because JS regexes have backreferences and
lookbehind that `regex` deliberately excludes. This is the same shape
as the `ryu-js` finding: the expensive part is an existing crate, so
the cost question is "does it fit the constraints", not "can we write
one".

Two constraints to check before adopting it, neither yet checked:
`regress`'s `utf16` feature is aimed at JS's UTF-16 strings while this
language stores UTF-8 (Q5), so the index domain has to be settled; and
§0.2 requires one implementation behind an opaque `sub_rt_*` symbol on
both tiers, which a crate satisfies as long as the ship tier links it
rather than emitting anything.

**Both checked and resolved in favour of adoption; shipped as P23
(`stdlib.md` §15, Q31, `specs/tracking/p23-regex.md`, 2026-07-27).**
The index domain needed no conversion — `regress` matches UTF-8
natively and returns byte offsets, which is Q5's domain, and the
`utf16` feature stays **off**. A third constraint this section did not
anticipate turned out to be the real one: `regress` has no execution
budget at any version, and an unbounded match is a hang the host cannot
interrupt, so the engine is a fork that adds one.

**Regex is therefore no longer "wanted, unscheduled".** The iterator
protocol below still is, in part: P22 delivered `for…of` over the
built-in containers and array-literal spread (Q30, §14), so what
remains is the *binding* — a spelling for "this type is iterable",
`keys`/`values`/`entries` returning iterators, and construction from an
iterable (`new Map([[k, v]])`). The memory-model question this section
raises — a stateful iterator outliving the call that made it, against
C5's non-escaping callbacks — is untouched and is still the hard part.

## Undecided — the two that are not simple

- **`flat`/`flatMap`.** The depth appears in the result type, so
  `flat(depth)` with a runtime depth cannot be typed. A depth-1-only
  form is implementable; whether a partial API is better than none is
  not decided.
- **`String.fromCharCode`.** Takes UTF-16 code units, and a lone
  surrogate has no UTF-8 representation. Accepting only the
  non-surrogate range would be a silent divergence from JS rather than
  a subset.

## How to keep this current

Every rejection carries one of the reasons above. A rejection recorded
with any other reason — "not in v1", "redundant", "JS-number op" — has
not been checked against the rule and should be.
