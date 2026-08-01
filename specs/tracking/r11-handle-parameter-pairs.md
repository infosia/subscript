# §34 — parameter-position handle-element pairs: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§34 — the last cell of the pair matrix, and the closure of the
R7.2 misleading-mirror class at parameter position.

## §34.3 evidence (reviewer-run)

1. `a107-interop-handle-parameter-pair` byte-identical under both
   tiers (leading handle parameter + handle array, count and
   per-element identity evidence).
2. The evidence signature reproduced live by the reviewer:
   `gpuQueueSubmit(queue: GpuQueue, commands: GpuCommandBuffer[])`
   — no count, no `| null`.
3. The class is closed everywhere: parameter recognition is total
   for pair-looking positions; registered enum/struct elements at
   parameter position now fail loud ("supported only at struct
   level") instead of emitting the split; mutable handle pairs fail
   loud as input-only; the audit is now
   `every_emitted_array_position_is_a_collapsed_pair_at_any_depth`.
4. No existing golden moved; gate 48 harnesses, 797 passed, exit 0,
   read directly; `tsc` exit 0; generated-docs gates green.

## Implementer decisions recorded

The collapse rides the existing scalar-pair provenance/lowering
path (`@subscript-c-scalar-pair` records with a handle element);
`SubDevice` is reused as the fixture handle; enum/struct parameter
pairs were NOT widened beyond §34 — fail-loud until evidence.
