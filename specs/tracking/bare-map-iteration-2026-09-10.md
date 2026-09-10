# A bare `Map` broke invariant 5, in two forms

Status: **landed 2026-09-10** as `specs/blocks/compiler.md` §104,
with §105 in the same implementation commit.

Origin: §103.5 recorded `[...map]` as an open item, found while the
§103 contract was written. This round measured the same shape in
`for…of`, which is the form programs use.

## The measurement

At `3d03f80`, macOS 26.6.2 arm64, both production tiers, TypeScript
5.9.2, node v24.18.0.

| form | this language | stock `tsc` |
|---|---|---|
| `for (const k of m)` with `const n: i32 = k` | binds `K`, prints it | `TS2322` |
| `const ks: i32[] = [...m]` | `K[]`, prints the keys | `TS2322` |
| `for (const k of m.keys())` with `const n: i32 = k` | binds `K` | accepts |
| `const xs: i32[] = [...set]` | `K[]` | accepts |

Invariant 5 requires an accepted program to type-check under stock
`tsc`. Both `Map` forms accept programs that do not. A collision
record cannot hold this: a divergence describes different behaviour
on a program both systems accept.

## Why the corpus did not see it

`a77` writes `for (const key of map)` and interpolates `key`, and
`tsc` accepts an interpolated pair. `a81` writes `const mapKeys =
[...map]` with no annotation, so `tsc` infers `[i32, string][]`, the
language infers `i32[]`, and neither reports. Both entries are
`tsc`-clean by accident.

**The `tsc` gate checks the corpus, and the corpus avoided the
annotation.** A gate over programs that a round chose is not a gate
over the form.

## Why the rule change is forced

Three answers exist, and two are unavailable. Yielding pairs needs a
tuple type — the gap that keeps `entries()` and `new Map([[k, v]])`
out. Weakening invariant 5 contradicts a permanent invariant, and
editor tooling is what invariant 5 buys. Rejecting the bare `Map`
needs no new machinery, so it is the answer §104 takes.

## The capability this retires

The one-expression spelling for the keys of a `Map`. The supported
replacement is a loop with `push`. Measured: `[...m.keys()]` is
**S014**, because §14.1 accepts a view only as a direct `for…of`
subject, so it is not the replacement.

A view in spread-operand position fuses as a view in subject position
fuses and creates no escaping value. Restoring the one-expression
spelling is a small separate contract. §104.2 records it; this round
does not establish its cost.

## Found beside it

`compiler/src/ambient.rs` states at six sites that a view elsewhere
"would create a stateful iterator value that outlives its call".
§103.3 retired that reason. §104.4 requires the six messages to state
the current obstacle or none.

## Corrections to `REPORT.md` of 2026-09-10

A review of that report found four claims that did not hold. Each is
corrected there:

- `[...m.keys()]` was named as the migration for `a81`. It is S014.
- "Blocked on item D" cited the interpreter's generator storage for a
  tuple prerequisite. The two are unrelated.
- Object destructuring was called blocked by C1. C1 rejects
  structural substitution and gives an object literal no standalone
  type; it does not forbid a named field read from a class instance.
- `Array.isArray` was called statically known in every case, and
  `Array.of` blocked by the variadic prerequisite in every arity.
  A boundary-opaque `object` or a nullable form can need a runtime
  test, and `push` and `unshift` already ship as fixed-arity forms.

## What landed

Three implementer rounds and one Phase Review, alongside §105.

**§104.** A bare `Map` is S014 as a `for…of` subject and as an
array-literal spread operand, read off the resolved type, without
reading how the bound value is used. Both sites carry a divergence
variant, so neither form is rejected without an explanation. Three
sites in the repository held the retired form — `a77`, `a81`, and one
`lib.rs` test — and a fourth sweep by the reviewer found no others.

**§104.4.** Six messages and a seventh string, the
`Divergence::IteratorTemporary` `why`, carried the reason §103.3
retired. `compiler/tests/retired_reasons.rs` is the total check: three
parts, no named-site list, each with its own non-empty guard, and a
permanent control that fires when a part reads less.

**§104.4a.** `Type::iteration_element`'s `Map` arm, `IterKind::MapKeys`
and `SpreadKind::MapKeys` are gone, so the class is unreachable rather
than guarded. No unexpected consumer broke.

**§105.** `Array` resolves as a builtin namespace in value position.
`Array.from(source)` lowers to `ExprKind::ArraySpreadLit` — the same
node, LIR instruction and runtime traversal as `[...xs]` — so §105.2's
rules 2 to 4 hold by construction and the runtime gained nothing.

**Counts**, `3d03f80` → here: accept `.ts` 217 → 219, accept
`.expected` 216 → 218, reject `.ts` 188 → 197.

**Gate**, on the final tree:

    gate full f36c573 dirty:34 debug 1410/0/2 release 1408/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 1 exit 0

`goldens-moved 1` is `a81-array-literal-spread.expected`. The LIR text
golden did not move: its selection is `is_generator || is_async` plus
a seven-name allowlist, and no new entry qualifies.

## The Phase Review

0 CRITICAL, 2 MAJOR, 9 MINOR. The reviewer ran about 30 `Array.from`
shapes on both tiers against node, 14 probes on the narrowing, rebuilt
binaries from both pins for the Red claims, and swept the blast radius
again. **No correctness defect was found**, and every finding was a
test, a comment, a diagnostic, or the contract.

Both MAJORs were the contract's. §105.4 named three `specs/` sites and
none was updated, because the implementation round may not edit
`specs/` and the planner missed them. §104.6 asked a test to "record"
the typed form's `tsc` code, and a round wrote
`let tsc_code = "TS2322"; assert_eq!(tsc_code, "TS2322")`, which
cannot fail — §79 rule 6 routes that fact to a test, so it was the
only pin the form had.

## Recorded, open, unscheduled

`§79` rule 6a. Rule 4's total gate reads the reject corpus, so an
`ambient` rejection row that no entry pins carries no divergence and
nothing reports it. Twelve `tsc`-accepted forms are measured rejected
with no block, most of them older than §105.
