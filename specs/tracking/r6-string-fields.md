# §28 — string-view fields in boundary structs: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§28. Origin: downstream report R6 — **accepted as a bug report**:
accept-and-miscompile against invariant 1, shared by both tiers and
therefore invisible to the differential gate (agreement is not
correctness; this is the recorded instance).

## The bug, pinned before the fix (Red)

The pre-fix reproduction — the exact downstream shape, a string-view
field first and scalars after — returned `7 / 0 / 42 / 9001` where
`706 / 8 / 0 / 42` was correct, identically under both tiers: every
field after the string was read shifted, because the lowering placed
the string handle in the struct storage instead of the 16-byte
`{data,len}` view.

## §28.3 evidence (reviewer-run)

1. `a97-interop-string-field-write` (script→C: scratch struct, view
   expansion, scalars at C offsets) and
   `a98-interop-string-field-read` (C→script: view materialized as
   an owned string, all-zero view as `""`) byte-identical under both
   tiers; goldens re-read locally and matched.
2. Fail-loud tests: by-value parameter and return, arrays of
   string-field structs, and count+pointer arrays of them, all
   rejected at bind time. Audit test
   `every_mirror_accepted_string_field_position_has_a_lowering`
   (bindgen/tests/provenance.rs) pins accepted ⇒ lowered.
3. The downstream shape is covered verbatim (§28.3.3).
4. No existing golden moved; gate 48 harnesses, 760 passed, exit 0,
   read directly; `tsc` exit 0.

## Implementer decisions recorded

Boundary structs containing strings are GC-rooted correctly (found
during the fix); adjacent `size_t` + pointer-to-string-field-record
parameters are conservatively classified as arrays and rejected;
`a98` is a separate entry for readability.
