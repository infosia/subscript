# §46 — narrowing flows into conditional arms

Status: **landed and verified 2026-08-03** against `compiler.md`
§46. Origin: downstream request R19, blocking the generator change
§45 was meant to enable — with §45 alone the conditional form was
reachable for only 1 of its 5 converters.

## What §45 missed, and why the corpus did not catch it

§45 gave the conditional its contextual type but not the flow facts
its condition establishes, so `if (v !== null) { use(v) }` was
accepted while `v !== null ? use(v) : 0` was rejected (S005) on a
plain reference class — reproduced at the pin, `tsc`-clean.

Recorded once, because this is the second instance of one gap:
§45's corpus exercised the conditional's *shape*, but every case
constructed its value inline, so no case reached a nullable
**local** — exactly as the OBS-3 corpus exercised descriptor shapes
but never a *returned* descriptor. **A corpus entry pins a
construction, and a construction is not a flow.** The reviewer's
own R18 report compounded it: its worked example used a predicate
over a non-nullable value, which is the one converter shape that
needs no narrowing.

## §46.3 evidence (reviewer-run)

1. `a125-conditional-arm-narrowing` byte-identical under both
   tiers: reference class, opaque handle, and the generator's real
   shape (`x !== null ? toX(x) : null` supplying a nullable
   boundary aggregate from a nullable local), both condition
   orders, both paths.
2. `r120-narrowing-escapes-conditional` pins that a path narrowed
   inside an arm is not narrowed after the expression. `tsc`
   finding recorded: TypeScript **also** rejects it (TS2345), so
   this is an agreement pin, not a strictly-narrower one.
3. Reject sweep re-pinned all 116 entries, the prior 115 at their
   recorded code and line.
4. Reviewer live probes at the landing: the R19 reproduction runs
   `7/9/0/0` (both forms, both paths); using a narrowed path after
   the conditional still rejects S005.
5. Gate 50 harnesses, 875 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; golden sweep 125 entries; no existing golden
   moved.

## Implementation note

No new narrowing analysis: `narrow_paths` became module-visible and
its existing then/else facts are applied around the conditional's
arms with the same scoped invalidation `check_if` uses. The change
is 20 lines in `check/expr.rs` plus one visibility change.
