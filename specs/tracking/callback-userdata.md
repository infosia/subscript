# Callback userdata — rooted, checked at fire, advised at free

Status: **landed and verified 2026-07-30** against `compiler.md`
§14.4b. Contract committed first; every exit criterion below was
re-run by the reviewer.

## What the mechanisms cost, measured by construction

The design premise was the asymmetry: binding records (§14.4a)
already hold the userdata pointers, so this pattern needs no
retention. What landed matches:

- **(C) rooting** — the mark seed extends from `Context::callbacks`
  at collect time (`context.rs`, the `for binding in &self.callbacks`
  walk beside the existing root and shadow seeds); null and non-live
  slots are skipped. No new per-Context field exists.
- **(A) fire check** — `validate_callback_userdata` in the
  trampoline, before any allocation or script entry: dead-set first
  (mode on → trap attributed to the freed allocation's site),
  live-map second, best-effort trap otherwise. New stable kind
  `callback-userdata-freed` (21).
- **(B) advisory** — `advise_callback_userdata_free` returns before
  the binding scan when no observer is installed; the observer API
  and `SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE`
  are in the regenerated header.

## §14.4b exit criteria — reviewer-run evidence

1. `corpus/accept/a90-callback-userdata-rooted` (register → drop
   refs → collect → pump → read fields) runs byte-identical under
   both tiers in the gate; its golden came from the standard capture
   path (new opt-in `capture-interop` feature links the fixture for
   capture only; `codegen/build.rs` is repo-relative).
2. `corpus/trap/t46-callback-userdata-freed` pins
   (`callback-userdata-freed`,
   "callback userdata points to a freed allocation", 31:43) under
   both tiers, diagnostics mode on, t22/t23's gating class.
3. Advisory unit and FFI tests green
   (`diagnostics_observer_advises_on_callback_userdata_free`, both
   layers); `diagnostics_observer_unset_has_zero_change` plus the
   full gate green with no committed golden moved is the zero-cost
   proof.
4. `callback_userdata_rooted_survives_collect` (dev and ship
   Contexts) and `callback_userdata_freed_slot_is_skipped_at_mark`
   green.
5. No tracked `.expected` changed (the §14.4b pre-registered audit
   predicted this: no prior program registers a sink and collects);
   `tsc` gate exit 0 with both new entries; gate 710 passed, exit 0,
   read directly.

## B2 + W003 — the fresh-userdata-per-registration pair, 2026-07-30

Landed together against §14.4b (B2) and `warnings.md` W003.
Reviewer-run evidence:

- `set_binding_count_advisory` defaults to `u64::MAX`, literal
  semantics; `advise_binding_count` is called only on the new-record
  path of `bind_callback` (after the intern miss — confirmed at the
  call site), so a re-registered identity can never advise. Message
  format `callback bindings: N registered, advisory threshold T`;
  threshold 0 advises on the first record (both unit-tested, plus
  FFI).
- W003 rendering reproduced locally on
  `corpus/warn/w03-fresh-callback-userdata-loop.ts` (17:35, the
  aggregate construction); detection keys on boundary HIR callback
  provenance, not class names. `a90` (one registration, no loop)
  stays silent with the clean line — reproduced.
- Zero-warning sweep: 91 accept + 10 example files clean. Gate 714
  passed, exit 0, read directly; `tsc` exit 0.

## Implementer decisions recorded

Fire-time position for a retained-dead slot is the freed
allocation's site (`pos_id` from the retained header), not the free
or pump site; non-owned/never-live addresses report position 0.
"Live binding" is every record in `Context::callbacks` — records are
Context-lifetime (§14.4a), so no liveness filter exists to apply.
