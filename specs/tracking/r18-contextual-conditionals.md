# §45 — contextual typing for conditional expressions

Status: **landed and verified 2026-08-03** against `compiler.md`
§45. Origin: downstream request R18 (non-blocking) — an optional
boundary-aggregate member forces its generator to emit 2^n
constructor calls for n such members, because the aggregate cannot
be produced by a helper the way a handle can.

## Two answers, and the second was wider than the request

**The naming restriction is deliberate** (§45.1). A boundary
aggregate is a value class (C2); C7 admits `Struct | null` only at
boundary positions, where `null` has a defined lowering — the
zeroed struct, or `NULL` for a §33 reach-through member. A script
local has no such representation. Widening it would give value
types a nullable representation in script, the same addition C2
still defers for nullable fields inside value classes. Handles are
already pointers, which is why the generator's
`toNullableSGPUBuffer` helper shape is legal for them.
`is_reference_shape() && !is_value_class()` is unchanged, and a
value-class union still rejects S011 (reviewer-probed after the
landing).

**The cost was removable, and the defect was general.** The
alternative the request named — building one argument conditionally
— did not work because `check_cond` passed the contextual type to
both branches but typed the conditional from the **then** branch
and required the else branch to be assignable to it. Measured at
the pin before contracting:

```ts
const c: C | null = flag ? new C() : null;   // rejected; tsc-clean
```

for an ordinary reference class, with the branch order merely
swapping which side was reported. Every `X | null` conditional in
the language was affected; the boundary aggregate was one instance.

## §45.4 evidence (reviewer-run)

1. `a124-contextual-conditional` byte-identical under both tiers:
   nullable reference class, nullable handle, and a nullable
   boundary aggregate as a conditional constructor argument, both
   branch orders, null and non-null paths observed.
2. `r119-conditional-without-context` pins the unchanged
   no-context rule (S100 at the null else branch); `tsc`-clean
   standalone, recorded in its header — another strictly-narrower
   pin.
3. Existing C7 pins unchanged: the S011 value-class-union rejection
   still fires at its code and line, now also held by an
   interop-mirror test, and the reject sweep re-pinned all 115
   entries.
4. Reviewer live probes at the landing: the shape that motivated
   the contract (`const c: C | null = flag ? new C() : null`) runs;
   a `@CStruct` value-class union still rejects S011.
5. Gate 50 harnesses, 869 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; golden sweep 124 entries both tiers; no existing
   golden moved.

## Implementer decision recorded

The ship tier needed one companion change: a nullable boundary
aggregate's value branch keeps its storage outside the branch so
the emitted pointer stays valid past the conditional — the same
lifetime discipline §44.9 established for returned boundary values,
applied to a new construction site.
