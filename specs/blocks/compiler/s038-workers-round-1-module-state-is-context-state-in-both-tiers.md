<!-- §38 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 38. Workers round 1 — module state is Context state in both tiers

Owner decision 2026-08-02. First of three rounds toward the Workers
model (host-owned threads, one Context per thread, runtime message
channels; the Q35 register entry lands with the final round). This
round is the isolation prerequisite and stands on its own.

Grounding, from source: the dev tier reaches every module global
through the Context-owned block (`lower/mod.rs`'s contract — "module
globals live in a host-owned block reached through the Context";
the ABI slot is `Context::globals_offset()`, offset 16), while the
ship tier emits each module global as a process-wide C `static`
(`cemit.rs` global emission). Sequential multi-Context use never
observes the difference because `subscript_init` reinitializes, but
two **concurrent** Contexts of one program image would share and
race ship-tier module state while dev-tier state stayed
independent — a tier divergence the differential gate cannot see,
because it drives one Context. The R6 lesson applies: tier
agreement on the existing corpus is not correctness.

### 38.1 Rule

**Module state is Context state, in both tiers.** The ship tier
emits module globals as one layout-fixed block allocated per
Context during `subscript_init` and reached through the same
Context slot the dev tier uses; a `static` definition for
language-visible module state is forbidden in emitted C. Immutable
data (string literal bytes, lookup tables) stays shared. A Context
is thread-affine: created, driven, and released wholly on one
thread (§14.6's single-threaded contract, restated for the
multi-Context case); cross-thread migration of a live Context
remains uncontracted.

### 38.2 Gates

1. A concurrency harness, both tiers: one program with mutating
   module state runs in two Contexts on two OS threads
   concurrently; each Context's per-Context stdout capture equals
   the single-Context golden byte-exactly. Ship tier: one compiled
   image, two Contexts — the case today's `static` emission fails.
   Dev tier: one session per thread. Headless, deterministic (each
   thread joins before comparison; no cross-thread ordering is
   asserted).
2. An emitter unit test: emitted C for a program with module
   globals defines no `static` module-state storage.
   *(Strengthened 2026-08-02 by the arc's Clean Review: the
   name-based test pinned the probe global and missed a second
   mutable-static class — see 38.3. The gate is now an audit:
   emitted C contains no mutable `static` definition of any kind;
   immutable rodata is whitelisted explicitly.)*

### 38.3 Review finding — capturing-lambda environments (2026-08-02)

The arc's no-context review found the ship tier still emitting one
mutable-static class §38.1 forbids: every capturing lambda's
environment (`static EnvL<n> …;` at its creation site), written on
every call, shared process-wide. This was wrong before Workers in
plain single-threaded programs — a function that creates a
capturing lambda, recurses, then calls the lambda reads the
recursive call's environment (dev tier 3, ship tier 0 — verified
live by the reviewer) — and Workers make the concurrent case
routine. C5 (captures never escape their defining function) is
exactly the property that makes an automatic-storage environment
sound; the `static` was never load-bearing.

Fix contract: the environment becomes function-local automatic
storage; `a114-lambda-env-recursion` pins the recursion pattern
Red-first (the pre-fix ship divergence is recorded, then the entry
lands with the fix); the 38.2-2 audit above pins the class. Also
from the same review, runtime-side: `subscript_rt_globals_init`'s
size/align conversion-failure arms must trap before returning null
(today they return bare null and emitted `subscript_init` returns
with no trap recorded — the harness then proceeds and crashes on
the null globals slot; reachable only off 64-bit hosts, fixed
loudly anyway).
3. Existing goldens byte-identical (single-Context semantics are
   unchanged); full gate and `tsc` gate green.
4. The standing ship-tier benchmark re-measured and the ratio
   recorded in `specs/tracking/workers.md` — global access gains an
   indirection; the cost is measured and reported, not assumed.
