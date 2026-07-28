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
10 000).

**Corrected 2026-07-29, before any measurement.** This section first said
retention is `reserved_bytes − live_bytes`, "which in the dev tier is
exactly the retained-dead set". That is wrong, and the code says so:
`live_bytes` sums each live allocation's **requested payload**, while
`reserved_bytes` sums each allocation's **layout**, header included
(`runtime/src/context.rs`). So

```
reserved − live = (header and padding on the live set) + (retained layout)
```

— the difference **overstates** retention by the live set's per-allocation
overhead. Measured on the existing accounting test, a dev run reports
`live = 8, reserved = 60` for a handful of 4-byte payloads: the two are
not equal even with nothing retained.

**The criteria below are unaffected**, because they were always derived
from *growth per frame*, and a constant live set contributes a constant
offset that the slope removes. What changes is what may be claimed: the
probe reports the **slope** — retained bytes per frame — as the measured
quantity, and reports the raw `live_bytes` and `reserved_bytes` at each
frame count beside it so the offset is visible rather than folded away.
An absolute "retained right now" figure is not claimed, because no
accessor yields one.

*(Found by the implementer, refusing to write a unit test whose stated
assertion — allocate-only means `reserved == live` — the accounting
contradicts. The author of this file had read that code an hour earlier
and wrote the wrong sentence anyway.)*

Reported per variant: frames, live bytes, reserved bytes, and the derived
**growth per frame** and **growth per allocation** — derived from the
change in `reserved_bytes` between frame counts, which is the slope the
correction above leaves valid. Live bytes must stay flat across frame
counts; if it does not, the probe is measuring the wrong thing and the run
is void.

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

## Result — measured 2026-07-29

Both variants, dev tier, one `i32` field per allocation:

| frames | live_bytes | reserved_bytes | growth/allocation |
|---:|---:|---:|---:|
| 0 | 0 | 0 | — |
| 100 | 0 | 2 000 | 20.000 |
| 1 000 | 0 | 20 000 | 20.000 |
| 10 000 | 0 | 200 000 | 20.000 |

Three findings, each measured rather than argued:

1. **Growth is exactly linear in cumulative allocations.** The slope does
   not move across two orders of magnitude.
2. **`Context.free` and `Context.collect` retain identically** — 20 bytes
   per allocation either way. In the dev tier the spelling of release
   changes nothing about retention. §8.1a says so; this is the first time
   it was measured.
3. **The live set held at 0 across every frame count**, so the measurement
   is of retention and not of ordinary growth. The validity condition the
   pre-registration set is met.

At this allocation shape — 4 bytes of payload, the smallest the language
can make — the reference budget lasts **1.85 hours**.

## Decision — owner, 2026-07-29

**The pre-registered criteria are superseded.** They asked how long the
budget lasts; the owner rejects the **shape** rather than the magnitude:

> Memory that grows linearly and without bound is not acceptable
> regardless of duration.

That is a stronger rule than any of the three thresholds, and it settles
the case without the object-size sweep the duration question would have
needed: at every object size the growth is unbounded and linear, so no
size makes it acceptable. Option 1 of the three listed below — accept and
document — is ruled out.

**What this reopens.** P24 §22.2 declined to bound the poison window,
preferring "a smaller, correct win … to a larger one that quietly narrows
a guarantee", and said reopening needs a measurement to justify the
narrower guarantee. The measurement above is that justification, and the
decision above is the owner's.

**What the retention actually buys, stated precisely, because the fix
depends on it.** Retaining the bytes is not about the payload — it is
about keeping the *address* out of circulation. A stale handle traps
because its address is still recognized as a dead allocation; if the
storage were released, the system allocator could hand that address to a
later allocation and the stale read would find a live object instead of a
trap. So any bound on retention is a bound on **how far back a
use-after-free is still detected**, and that is the quantity a design
response trades away. It must be stated in those terms rather than as a
memory setting.

Mechanism is not yet chosen; that is the next decision.
