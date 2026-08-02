# §42 — divergence flow: exhaustive switches and `unreachable()`

Status: **landed and verified 2026-08-02** against `compiler.md`
§42. Origin: downstream R15 (15.1 blocking its P5 slice E1; 15.2 a
design question, answered as a decision).

Grounding recorded before contracting: the R15.1 function shape and
`declare function unreachable(): never` are both stock-`tsc`-clean
(`--lib es2022` standalone, exit 0) — `tsc`'s own flow analysis
already accepts the exhaustive switch and treats the `never` call
as diverging, so §42 aligns this compiler with what `tsc`
concludes. Design decision on 15.2: first-class `unreachable()`
over the bounds-check idiom — intent-explicit and general to any
generated code; trap kind 23 under C6.

## §42.4 evidence (reviewer-run)

1. `a116-exhaustive-switch-returns` byte-identical under both
   tiers (the HANDOFF's `lower()` shape verbatim — no trailing
   return — plus a tail `unreachable()` after early returns).
2. `t47-unreachable-reached`: kind 23 at the call site under both
   tiers in the trap differential (47 entries), stdout `before`
   pinned before the trap.
3. `r115-unreachable-as-value` pins; implementer probe: `tsc`
   exit 0 (TypeScript 5.9.2 permits `never` in value positions) —
   a strictly-narrower `tsc`-clean pin, recorded in its header.
4. Reviewer live probes at the landing: the combined HANDOFF-shape
   program (`lower` + `mapState` with tail `unreachable()`) runs
   `3:pending` under `subscript run`; value-position `unreachable`
   rejects with "`unreachable()` is only legal as a call
   statement".
5. Gate 48 harnesses, 842 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; accept differential 116 entries, 0 skipped; no
   existing golden moved; zero-warning and generated-docs gates
   green.

## Implementer decisions recorded

No general-purpose `never` type was added: the ambient's `never` is
the `tsc` view only; the checker keeps `unreachable()` as a
call-statement-only non-value operation, and the flow analysis
keys the switch-divergence rule off `Type::StringAlias` +
`default`-less directly (§41's guarantee), with nested exhaustive
switches handled recursively. Enum/integer/string switches keep
their conservative flow behavior, unit-pinned.
