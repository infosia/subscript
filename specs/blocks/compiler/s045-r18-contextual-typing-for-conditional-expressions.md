<!-- §45 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 45. R18 — contextual typing for conditional expressions

Owner decision 2026-08-03 (downstream request R18, non-blocking).
The request asked whether a boundary aggregate may be named as
`T | null` in script positions, and — if not — for "any way to
build one constructor argument conditionally without restating the
call", because an optional aggregate member currently forces the
generator to emit 2^n constructor calls for n such members.

### 45.1 The naming restriction is deliberate

A boundary aggregate declared by `subscript bind` is a **value
class**: C layout, copy on assign, copy on pass (C2). C7 admits
`Struct | null` only at boundary positions, where `null` has a
defined lowering — the zeroed struct for a by-value sub-layout, or
`NULL` for a §33 reach-through pointer member. In a script local,
parameter, or return type there is no such representation: a value
class has no pointer to be null. Naming it would mean giving value
types a nullable representation in script, which is a semantic
addition, not a checker oversight — the same reason C2 still defers
nullable fields *inside* value classes. Handles differ because they
already are pointers, which is why the generator's
`toNullableSGPUBuffer` helper shape is legal for them and stays so.
The restriction stands; `is_reference_shape() && !is_value_class()`
remains the rule.

### 45.2 The cost is removable, and the gap is general

The alternative the request named is the right one, and the reason
it does not work today is a defect wider than the request.
`check_cond` passes the contextual type to both branches but then
takes the **then branch's** type as the conditional's type and
requires the else branch to be assignable to it. So

```ts
const c: C | null = flag ? new C() : null;
```

is rejected for an ordinary reference class — measured at the pin,
`tsc`-clean — and swapping the branches merely swaps which side is
reported. Every `X | null` conditional in the language is affected;
the boundary case is one instance.

Rule: **when a conditional expression has a contextual type, that
type is the conditional's type and each branch is checked against
it.** With no contextual type the existing rule stands (the else
branch must be assignable to the then branch's type). Nothing else
about conditionals changes.

This removes the request's 2^n without widening C7: one constructor
call with n conditional arguments, `cond ? toSGPULimits(x) : null`
in a boundary parameter position, where `Struct | null` is already
legal.

### 45.3 Corpus

`a124-contextual-conditional` (accept): nullable reference class,
nullable handle, and — through the interop fixture — a nullable
boundary aggregate supplied as a conditional constructor argument,
in both branch orders, with the null and non-null paths both
observed. Reject: `r119-conditional-without-context` pins that an
uncontextualized conditional over mismatched branch types is still
rejected (`tsc` status recorded), and the existing S011 pins for
naming a value-class union stay as they are.

### 45.4 Exit criteria (pre-registered)

1. `a124` byte-identical under both tiers.
2. `r119` pins (code, line) with its `tsc` status recorded; no
   existing reject entry changes meaning.
3. Checker unit tests: contextual conditional in each branch order;
   nested conditionals; no-context conditional unchanged; a
   conditional whose branches are both assignable to the context
   but not to each other.
4. `tsc` gate green with `a124`; no existing golden moves; full
   gate green; zero-warning sweep green.
