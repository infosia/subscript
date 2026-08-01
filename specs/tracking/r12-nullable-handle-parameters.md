# §35 — `_Nullable` handle parameters: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§35 — the §31.2 field rule at parameter position.

## Evidence (reviewer-run)

1. `a108-interop-nullable-handle-parameter` byte-identical under
   both tiers (live-handle and `null` spellings; checker codes
   0 = NULL, 1 = same-as-encoder, 2 = distinct live handle).
2. The evidence signature reproduced live:
   `gpuSetBindGroup(encoder: GpuEncoder, group: GpuBindGroup | null)`.
3. The honored-position set grew by exactly one; the fail-loud
   "only ..." text now names all three honored positions (handle
   fields, boundary-struct pointer fields, handle parameters), and
   the §31 fail-loud suite passes with the updated text.
4. No existing golden moved; gate 48 harnesses, 801 passed, exit 0,
   read directly; `tsc` exit 0; generated-docs gates green.

## Implementer decisions recorded

Minimal diff: both tiers' existing scalar-pointer lowering already
carries a null handle as `NULL` at parameter position, so only
bindgen's honored-position validator was extended — no codegen
change. `SubDevice` reused for both fixture roles.
