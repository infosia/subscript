# §32 — recursive lowering at embedded positions: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§32. Origin: downstream request R9 — §30.1's recorded boundary
("embedded aggregates containing absorbed members") arriving with
its evidence, resolved by the contracted generalization:
"recursively plain" widened to "recursively lowered".

## §32.3 evidence (reviewer-run)

1. `a103` (embedded string field), `a104` (the composed
   render-pipeline depth chain: descriptor → vertex state → buffers
   pair → buffer-layout element → attributes pair, checker walking
   the full depth), and `a105` (string-field-struct pair elements —
   landed same round, no lag) byte-identical under both tiers.
2. All three verbatim evidence rejections from pin `2016bf0` now
   mirror and lower, bindgen-unit-tested on those shapes; the
   reviewer additionally probed a deeper composition live (a string
   view inside the embedded aggregate beside a pair whose element
   contains another pair) — mirrors correctly.
3. Audits extended to any depth:
   `every_emitted_struct_array_field_is_a_collapsed_pair_at_any_depth`,
   `every_mirror_accepted_string_field_position_has_a_recursive_lowering`;
   recursive readback stays rejected with diagnostics descending to
   the innermost absorbed member.
4. No existing golden moved (105-entry differential gate green);
   gate 48 harnesses, 788 passed, exit 0, read directly; `tsc`
   exit 0; generated-docs gates green.

## Implementer decisions recorded

"Pair elements" applies uniformly, including descriptor parameters;
recursive scratch arrays use nested call-scoped allocations so they
stay valid through synchronous and re-entrant foreign calls.
