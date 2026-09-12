<!-- §31 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 31. R8 — opaque handles in aggregate positions

Owner decision 2026-08-01 (downstream request R8, blocking its
bind-group area).

### 31.1 R8.1 — handle elements in count-first pairs

The pair-element set (§30.2) extends to registered opaque handles:
an adjacent `size_t <name>Count, const H* <name>` where `H` is a
registered handle typedef collapses to `<name>: H[]`, count elided —
input direction only in v1 (a mutable handle-array direction is a
new request with evidence). Elements are pointer-sized values; the
language array's storage crosses as the `const H*`.

### 31.2 R8.2 — nullable handle fields via `_Nullable`

The mirror honors clang's `_Nullable` qualifier on **handle fields
of boundary structs**: the field mirrors as `H | null`; script
`null` lowers to `NULL`, and on the read direction a `NULL` field
reads as `null`. The unqualified handle field stays non-null, as
today. Chosen over a provenance directive because the frontend is
already libclang, the header stays self-documenting, and the
downstream facade header is generated and can emit whatever spelling
the contract names.

The silent-ignore mode the probe found is closed the §30 way:
`_Nullable` on any position without this lowering — non-handle
fields, parameters, returns, pair elements — **fails loud** naming
the position, so a qualifier the mirror does not honor can never be
dropped silently again.

### 31.3 Corpus

`a101` (accept): the pipeline-layout shape verbatim — label view +
handle-element pair — passed script→C to a fixture checker that
returns per-element identity evidence. `a102` (accept): a
bind-group-entry shape with three `_Nullable` handle fields, script
setting exactly one non-null per entry, the checker reporting which;
plus the read direction (C fills one of three). Bindgen unit tests:
handle-pair collapse; `_Nullable` handle field mirrors `H | null`;
`_Nullable` on an unsupported position fails loud.

### 31.4 Exit criteria (pre-registered)

1. `a101`/`a102` byte-identical under both tiers.
2. The downstream evidence shapes mirror exactly:
   `bindGroupLayouts: SGPUBindGroupLayout[]` (no count), and a
   `_Nullable` handle field as `H | null` — bindgen-unit-tested.
3. `_Nullable` anywhere unsupported fails loud (unit-tested); the
   §30 audit still holds (every array field a collapsed pair).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.
