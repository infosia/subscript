# Dev-tier retention — pre-registered measurement

Status: **pre-registered 2026-07-29, not yet run.** Criteria below are
fixed before the numbers exist.

## The question

The development tier realizes `Context.free` and `Context.collect` by
**retain-and-poison** (`compiler.md` §8.1a): freed bytes stay owned by the
Context with a dead header, so a stale handle traps instead of reading
reused memory. `runtime/src/context.rs` states the consequence exactly —
the retained-ownership vector is "walked only at Context drop, never by
collection" — and `compiler.md` §8.2 keeps Context state alive across a
hot reload, so retention accumulates over a whole editing session, not
one run.

**The cost was accepted without a bound.** §8.1a took it as the price of
the trap guarantee; P24 §22.2 fixed sweep *time* and said in terms that
memory retention "is unchanged — that cost §8.1a already accepted and
this phase does not reopen". Nothing in the specs or the benchmark suite
records how much memory that is per unit of work, and the `collect`
workload measures collection time rather than retention.

So the trade is recorded and its magnitude is not. A frame loop that
allocates per frame is the case where the two differ most, and it is the
case this language is built for.

*(Raised by the owner 2026-07-29, asking whether a game-shaped loop
exhausts memory in the dev tier. The arithmetic suggested it might. The
arithmetic is not a measurement, which is why this file exists.)*

## What is measured

A script shaped like a frame loop: each frame allocates a fixed number of
reference-class objects and then makes them unreachable, so the **live set
is constant** and only cumulative allocation grows. Two variants, since
both are legitimate spellings a host would write:

- **A** — each object released with `Context.free` in the same frame.
- **B** — references dropped, `Context.collect()` once per frame.

Run under the dev tier at several frame counts (at least 100 / 1 000 /
10 000). Retention is `sub_rt_ctx_reserved_bytes − sub_rt_ctx_live_bytes`,
which in the dev tier is exactly the retained-dead set
(`Context::reserved_bytes` sums live plus `retained_allocations`).

Reported per variant: frames, live bytes, reserved bytes, retained bytes,
**retained per frame**, and **retained per allocation**. Live bytes must
stay flat across frame counts; if it does not, the probe is measuring the
wrong thing and the run is void.

The extrapolation to a session is computed from the measured
retained-per-frame, not from an assumed object size.

## Pre-registered criteria

Against a reference workload of **1 000 allocations per frame at 60 fps**
in an **8 GB** budget — a plausible shape for the loop this language
targets, chosen now so the number cannot be picked to fit the result:

1. **≥ 8 hours before exhaustion** — acceptable. Record the number in
   §8.1a beside the accepted cost and close this file. A developer does
   not run one session longer than a working day.
2. **< 1 hour** — a finding. Playtesting for an hour is ordinary
   development, and a tier that cannot survive it is not a development
   tier for this domain. A design response is required, and this file
   records the options and the owner's choice.
3. **Between 1 and 8 hours** — the owner decides with the number in hand.
   No default.

**Ship tier is not re-measured here.** §8.1a contracts it as releasing,
and `examples/host/` already shows a host observing 1 664 → 176 bytes
across an explicit collection. If the dev probe's ship-tier cross-check is
cheap, it is reported as a control; it is not the subject.

## Options, listed before the numbers

So that a response is chosen on the measurement rather than invented under
its pressure:

- **Accept and document** — record the measured session budget and the
  guidance that follows from it (release the Context per scene, as
  `examples/context-per-scene/` shows).
- **Bound the poison window** — reclaim retained blocks older than some
  distance. P24 §22.2 explicitly declined this ("a smaller, correct win is
  preferred to a larger one that quietly narrows a guarantee"), so
  reopening it needs the measurement to justify the narrower guarantee.
- **Reuse retained blocks after a threshold** — trades trap-at-distance
  for memory; the same objection applies, with the same requirement.

Nothing here is adopted in advance.

## Result

Not yet run.
