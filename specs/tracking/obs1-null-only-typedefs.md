# §36 — emitted C names every referenced boundary typedef: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§36. Origin: downstream observation OBS-1 (non-blocking, accepted
as a bug): a boundary class referenced only in null position was
omitted from the ship tier's emitted typedefs — accept-then-fail-
late at the C step. No corpus entry had the shape; the gate could
not see it.

## Evidence (reviewer-run)

1. Red first: the pre-fix reproduction (null passed for
   `SubBoundaryStringRecord | null`, nothing constructed) failed
   clang with `unknown type name 'Sub_36_SubBoundaryStringRecord'`
   — the full error text is in the implementer's report. Notably,
   the first shape tried (`SubQueryStatus | null`) did NOT
   reproduce: a plain struct never references the internal typedef,
   so only scratch-lowered shapes expose the omission — recorded so
   future reproductions pick the right shape.
2. Fix: foreign-signature parameter/return types join the C
   emitter's aggregate reachability; dev tier untouched.
3. `a109-interop-null-only-boundary-reference` byte-identical under
   both tiers (output `18446744073709551615`), golden re-read by
   the reviewer; the 109-entry differential gate green.
4. The general pin: `every_generated_class_type_reference_has_a_
   typedef_definition` scans all emitted `Sub_<id>_<name>`
   references for matching definitions — the defect class, not the
   instance.
5. No existing golden moved; gate 48 harnesses, 802 passed, exit 0,
   read directly; `tsc` exit 0; generated-docs gates green.
