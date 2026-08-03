# §48 — external types in a generated mirror

Status: **landed and verified 2026-08-03** against `compiler.md`
§48. Origin: downstream request R20 (non-blocking) — its P6 host
header must reference `SGPUTextureView`, a handle another mirror
declares, and `subscript bind` had no way to emit that, so P6 was
hand-writing the engine mirror.

## What was wrong beyond the missing feature

The fail-loud diagnostic named two remedies — a mapping and a
typedef — and **neither existed**: there is no mapping parameter on
either entry point and no CLI option for one, while a typedef emits
either a colliding declaration or an independent brand that will
not substitute. A message that points at mechanisms the tool does
not have is its own defect, and it is now rewritten to name the one
that does.

## Design decision recorded

The directive lives **in the header**
(`/* @subscript-external SubDevice */`), not in a CLI flag or an
entry-point parameter. Reason: §12.2 requires the mirror to
regenerate byte-identically from `subscript bind <header>`; putting
the external set out of band would make that gate depend on
arguments no one can recover from the repository. The implementer
collects directives from the whole header, so placement before
first use is not required — recorded in bindgen's docs and tests.

## §48.3 evidence (reviewer-run)

1. `corpus/interop/external-device.h` carries
   `/* @subscript-external SubDevice */`; its generated mirror
   records `// @subscript-c-external type="SubDevice"`, references
   `SubDevice` at every use site, and contains **zero**
   declarations of it (reviewer-counted).
2. `a127-interop-external-type` binds both mirrors in one program
   and crosses the boundary through each, byte-identical under both
   tiers; the differential sweep compares 127 entries, 0 skipped.
3. The rewritten diagnostic names only
   `/* @subscript-external X */` — reviewer-read at
   `bindgen/src/emit.rs:1673`.
4. Bindgen tests pin the class: declaration suppression, an unused
   directive, external-plus-local-definition, and the new wording;
   regeneration byte-compare is green for both headers, and
   `interop.generated.d.ts` did not move.
5. Gate 51 harnesses, 885 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; no existing golden moved.

Resolution stays the program's: an external name no ambient file
declares raises the existing S100 unknown-type-name error one layer
down, rather than a bindgen-side check — fail loud, without a
silent brand.
