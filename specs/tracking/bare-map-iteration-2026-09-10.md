# A bare `Map` broke invariant 5, in two forms

Status: **contract landed 2026-09-10** as `specs/blocks/compiler.md`
§104. Implementation is open.

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
