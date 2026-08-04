# §49 — a host-driven ship-tier form

Status: **landed and verified 2026-08-04** against `compiler.md`
§49. Origin: downstream request R21.

## The finding was gate integrity, not a missing feature

The ship-tier runner compiles, links, and **spawns** a program
whose entry is `main`, so a host could not run code before the
script's entry there. Every suite program therefore had to keep its
long-lived state script-side, and the downstream's P6 review found
a use-after-free that **passed both tiers precisely because of that
inversion** — the runner's shape was selecting for the defect the
differential gate exists to catch.

Fourth instance of one pattern in this exchange (§44.9, §46, §47):
the harness exercised the constructs its own shape permitted, and
the defect lived where that shape could not reach.

## §49.5 evidence (reviewer-run)

1. `a128-host-owned-state` byte-identical under both tiers: the
   pre-entry hook creates the object, the script borrows it, and
   the golden shows `40` then `41` — the state advances across
   **two** entry calls, so its lifetime provably exceeds one call.
2. The contract's load-bearing property is pinned by name:
   `aot_entry_without_host_hooks_is_byte_identical_to_the_standing_entry`
   passes, and the `AOT_ENTRY_C` constant is untouched — the hook
   generator returns it verbatim when both hooks are `None`.
   Reviewer-verified by reading the diff and running the test.
3. Hooks are optional and independent (pre only, post only, both,
   neither) and non-identifier hook names are rejected — three
   unit tests, all passing.
4. **Stronger than contracted**: the fixture's create/destroy pair
   is host-only and absent from the generated mirror, which
   exposes only `subHostOwnedStateBorrow` and
   `subHostOwnedStateAdvance`. The script therefore *cannot* own
   the lifecycle — the ownership direction §49.1 fixes is enforced
   by the boundary rather than by convention.
5. Gate 51 harnesses, 888 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; differential sweep 128 entries, 0 skipped;
   regeneration byte-compare green; no existing golden moved.

## Implementer decision recorded

Both hooks run **unconditionally** with respect to the trap state:
the pre-entry hook precedes any script work, and the post-run hook
must run even after a trap so the host can always release what it
created. Neither is bracketed by `enter_script`/`exit_script`.

## Parked, now evidenced

The downstream's preferred shape — a session API mirroring
`ReloadSession` so one harness path drives both tiers — needs the
ship tier to emit a loadable library the host process opens rather
than an executable it spawns. That is the `run --native` loader on
the R3 parked list; R21 is its first real evidence. Recorded, not
scheduled: §49 restores the gate without it.
