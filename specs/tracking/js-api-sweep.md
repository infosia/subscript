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

## Accepted — contracted, implementation pending

These are decided and specified; the work is queued.

| API | contract | cost |
|---|---|---|
| `Number.prototype.toString(radix)` | Q26, `stdlib.md` §11.5 | ~120 lines |
| `Number.prototype.toExponential` | Q26, §11.5 | ~100 + ~90 shared |
| `Number.prototype.toPrecision` | Q26, §11.5 | ~125, shares the above |
| `Math.clz32` | Q26, `stdlib.md` §1 | 1 line |

Two normative traps are recorded in Q26, both places where the C tier's
obvious lowering is wrong: `Math.clz32(0)` is `32` while
`__builtin_clz(0)` is undefined, and ECMA does not pad the exponent
(`(0).toExponential(2)` is `0.00e+0`; C's `%e` gives `0.00e+00`).

## Accepted by the rule — not yet contracted

The sweep of 2026-07-25 found these fail no surviving reason: each
exists in JS, introduces no defect, and needs no prerequisite the
project lacks. **They are deferred only in ordering** — Q26 was already
contracted and is closed first (owner decision: finish the contracted
work before opening more). Nothing here is rejected.

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
| `Array` | `splice`, `shift`, `unshift`, `copyWithin` | |
| `Array` | **index/array parameters on callbacks** | `map((v, i) => …)`; the largest item here by real-world use, and the one that touches the checker's arity machinery |
| `Array` | the `every` family on `FixedArray` | currently a "v1 is `T[]` only" scope rejection |
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
