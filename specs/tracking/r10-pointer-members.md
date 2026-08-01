# §33 — lowering through struct-pointer members: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§33 — the final §32 recursion axis. Origin: downstream request R10,
the last blocker of its pipeline area.

## §33.3 evidence (reviewer-run)

1. `a106-interop-recursive-struct-pointer-members` byte-identical
   under both tiers, exercising null and non-null `fragment` and
   `blend` pointer spellings in one program with a checker reading
   evidence from every level behind both pointer kinds.
2. The verbatim `aeaffcf` rejection now mirrors and lowers
   (`recursive_render_pipeline_struct_pointer_evidence_shape`); the
   reviewer reproduced the composed reach-through live on a probe
   header (`fragment: Frag | null` → string + targets pair →
   `blend: Blend | null`).
3. Fail-loud coverage: read direction, mutable pointer targets, and
   unsupported pointer-reachable members all name the innermost
   member; **pointer/pair type cycles that cannot be finitely
   scratched fail loud** (`cyclic_pointer_reachable_lowering_...`)
   — a case the contract did not anticipate, adopted.
4. Audits extended over the pointer-reachable set
   (`..._at_any_depth` both); no existing golden moved (162/162
   hashes); gate 48 harnesses, 794 passed, exit 0, read directly;
   `tsc` exit 0; generated-docs gates green.

## Implementer decisions recorded

Explicit `_Nullable` on boundary-struct pointer fields is accepted
(qualified and unqualified both mirror `X | null`, as before).
Inside an aggregate whose scratch construction has begun, plain
pointer targets (e.g. `blend`) are also rebuilt as child scratches;
a struct with no absorbed lowerings anywhere (`SubChainHeader`)
keeps its original zero-copy pointer path. The full fragment
composition is a new fixture struct so a104's mirror did not move.
