<!-- §35 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 35. R12 — `_Nullable` handle parameters

Owner decision 2026-08-01 (downstream request R12, blocking its
pass encoders). The §31.2 field rule at parameter position:
`_Nullable` on a **registered opaque-handle parameter** mirrors
`H | null`, and script `null` lowers to `NULL` at the call.
Unqualified handle parameters stay non-null. The honored-position
set grows by exactly this one entry; every other `_Nullable`
position keeps §31.2's fail-loud, and the fail-loud message's
"only ..." list is updated to name both honored positions.

Corpus: `a108` (accept) — the set-bind-group shape: a leading
encoder handle plus a `_Nullable` handle parameter, called once
with a live handle and once with `null`, the checker reporting
which. Bindgen unit tests: the collapse on the verbatim evidence
signature; `_Nullable` on a non-handle parameter still fails loud.

Exit criteria: (1) `a108` byte-identical under both tiers, both
spellings; (2) the evidence signature mirrors
`(encoder: ..., group: SGPUBindGroup | null)` — unit-tested;
(3) the §31 fail-loud suite still passes with the updated message;
(4) no existing golden moves; full gate, `tsc`, zero-warning, and
generated-docs gates green.
