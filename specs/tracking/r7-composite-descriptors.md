# §30 — nested aggregates beside strings; struct-level pairs: evidence

Status: **landed and verified 2026-08-01** against `compiler.md`
§30. Origin: downstream request R7 (blocking its textures area).
R7.1 arrived as a clean §28-audit fail-loud (the rule doing its
job); R7.2 exposed the third failure mode — a *misleading* mirror
(leaked count, `Enum | null` for a `const Enum*` array) — now
impossible to emit.

## §30.4 evidence (reviewer-run)

1. `a99` (write: label view + embedded extent + collapsed enum pair
   + trailing scalars through one checker) and `a100` (read: C
   fills; strings materialize, aggregates copy back) byte-identical
   under both tiers — the read direction landed in the same round.
2. The reviewer probed `subscript bind` live on a composite
   descriptor: the full shape mirrors as
   `label: string; size: GpuExtent; viewFormats: GpuFormat[];
   usage: u32` with no count field; a plural-mismatched count name
   and an unregistered element each fail loud with messages naming
   the rule (`viewFormatCount` requires pointer field `viewFormat`;
   only lang_scalar/registered-enum/registered-struct elements).
3. Audit extended: `every_emitted_struct_array_field_is_a_collapsed_pair`
   plus the §28 audit covering plain embedded aggregates; fail-loud
   unit tests for non-adjacent and name-mismatched pairs.
4. No existing golden moved; gate 48 harnesses, 769 passed, exit 0,
   read directly; `tsc` exit 0; generated-docs regeneration gates
   green (corpus index includes a99/a100).

## Implementer decisions recorded

"Copy verbatim" means recursively plain C-layout aggregates;
aggregates containing absorbed descriptors, callbacks, strings, or
collapsed pairs stay fail-loud rather than byte-copied wrongly.
Both `<name>Count` and legacy `<name>_count` spellings collapse.
Mutable pair-element writes use the script array's backing storage
directly, so array handles stay intact on read-back.
