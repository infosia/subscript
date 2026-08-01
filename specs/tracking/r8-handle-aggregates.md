# §31 — opaque handles in aggregate positions: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§31. Origin: downstream request R8 (blocking its bind-group area).

## §31.4 evidence (reviewer-run)

1. `a101-interop-handle-array-pair` (label + handle-element pair,
   per-element identity evidence) and
   `a102-interop-nullable-handle-fields` (one-of-three `_Nullable`
   fields, both directions) byte-identical under both tiers.
2. The reviewer probed `subscript bind` live: the pipeline-layout
   shape mirrors `bindGroupLayouts: GpuBindGroupLayout[]` (no
   count), and `_Nullable` handle fields mirror `H | null`.
3. `_Nullable` silent-ignore is closed: seven fail-loud unit tests
   (non-handle field, parameter, return, pair element, value-class
   field, callback parameter/return); a mutable handle pair fails
   loud with "handles are input-only".
4. libclang nullability visibility recorded: prefix and suffix
   spellings both visible; `clang_Type_getNullability` returns
   Invalid through typedef sugar, so the frontend reads the cursor
   token stream plus type spelling — no valid spelling is invisible.
5. No existing golden moved (100/100 SHA match); gate 48 harnesses,
   781 passed, exit 0, read directly; `tsc` exit 0; generated-docs
   gates green.

## Implementer decisions recorded

The fixture reuses the existing `SubDevice` handle (no new handle
type); the exact downstream names are pinned by a dedicated bindgen
test instead. Identity evidence is first-occurrence position of the
same pointer, order-independent. Using `_Nullable` once makes clang
emit nullability-completeness warnings for every unannotated
pointer; the fixture suppresses that warning only — no mirror
semantics changed for existing pointers.
