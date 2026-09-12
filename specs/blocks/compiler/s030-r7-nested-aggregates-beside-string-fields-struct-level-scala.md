<!-- §30 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 30. R7 — nested aggregates beside string fields; struct-level scalar/enum pairs

Owner decision 2026-08-01 (downstream request R7, blocking its
textures area). Two gaps, one shared principle: **a mirror is either
lowered or loud** — §28.1's rule, restated because R7.2 shows a
third failure mode beside miscompile and fail-loud: a *misleading*
mirror (a leaked count field and a `const Enum*` array mirrored as
`Enum | null`) that type-checks and means the wrong thing.

### 30.1 R7.1 — aggregate fields in the §28 scratch construction

The §28 C-layout scratch extends to structs that mix string-view
fields with embedded boundary aggregates: an aggregate field copies
verbatim at its C offset (recursively — nested aggregates keep
their own layout), string fields expand to views as §28, scalars
copy as today. Script→C is the blocking direction and lands now;
the read direction gets the same treatment (aggregate copy-back
beside string materialization), and if the implementer finds it
disproportionate it may land immediately after — the §28 fail-loud
holds for whichever direction is not yet lowered.

### 30.2 R7.2 — struct-level count-first pairs, scalar and enum elements

The struct-level `<name>Count`/`<name>` adjacency rule (which
already collapses registered-struct descriptor pairs) extends to
`lang_scalar` scalars and registered u32-enum elements: the pair
mirrors as `<name>: T[]`, count elided, with the §27 semantics per
direction. And the hard rule: **any struct-level count+pointer
adjacency that does not collapse fails loud at bind time** — the
leaked-count shape from the downstream evidence must be impossible
to emit. The §28.1 audit test extends to pair positions: every
mirror-emitted array field is a collapsed pair, every uncollapsed
adjacency is an error.

### 30.3 Corpus

`a99` (accept): the downstream texture-descriptor shape verbatim —
`label` string-view field, an embedded all-scalar aggregate
(extent), a `viewFormats` enum pair, and trailing scalars — passed
script→C to a fixture checker returning every component; golden
proves the offsets and the pair collapse together. The read
direction, when it lands, extends `a99` or adds `a100`. Bindgen unit
tests pin the enum-pair collapse, the scalar-pair collapse at struct
level, and the fail-loud for uncollapsed adjacencies.

### 30.4 Exit criteria (pre-registered)

1. `a99` byte-identical under both tiers; the returned components
   prove aggregate offsets beside an expanded string field and the
   collapsed enum pair in one struct.
2. The downstream evidence shape (`SGPUProbeTextureDescriptor`)
   mirrors as `viewFormats: SGPUProbeFormat[]` with no count field;
   an uncollapsed adjacency (e.g. non-adjacent count) fails loud —
   both bindgen-unit-tested.
3. The extended audit test: every emitted array field is a collapsed
   pair; no mirror emits a bare count + pointer-as-nullable shape.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep green.
