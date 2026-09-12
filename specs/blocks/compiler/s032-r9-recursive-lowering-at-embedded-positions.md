<!-- §32 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 32. R9 — recursive lowering at embedded positions

Owner decision 2026-08-01 (downstream request R9, blocking its
pipeline area; this is §30.1's recorded boundary arriving with its
evidence). The generalization: **"recursively plain" widens to
"recursively lowered"** — the absorbed lowerings that exist at top
level apply at embedded positions.

### 32.1 The rule

In the script→C scratch construction, an embedded boundary
aggregate is built by the same rules as its parent, recursively:
string-view fields expand to views (§28), collapsed count/pointer
pairs emit `(count, ptr)` from the language array (§30/§31,
including handle elements), `_Nullable` handle fields lower `null`
to `NULL` (§31), plain scalars and plain aggregates copy at their C
offsets. The same recursion applies at **pair-element positions**:
a pair whose element type is itself a lowered boundary struct
crosses as a scratch **array** — each element built recursively —
valid for the duration of the call, like every §28 view.

Direction: script→C, per the evidence (no read-direction case
blocks). Reading any recursively-lowered position stays fail-loud —
the §28/§30 discipline — until evidence arrives. Whatever a
recursive position still cannot lower (an absorbed member with no
write-direction lowering of its own) stays fail-loud naming the
innermost offending member.

### 32.2 Corpus

`a103` (case 1): the compute-pipeline shape — descriptor embedding
an aggregate whose `entryPoint` is a string view. `a104` (case 2):
the render-pipeline depth chain composed end to end — descriptor →
vertex-state aggregate → buffers pair → buffer-layout element →
attributes pair — pinning the composition, not just the isolated
primitives. `a105` (case 3, may lag per the HANDOFF): a pair whose
elements are string-field structs (`constants` entries); if it
lags, its fail-loud stays and the lag is recorded in tracking, not
silent. All script→C through fixture checkers returning every
component; goldens by the standard capture path.

### 32.3 Exit criteria (pre-registered)

1. `a103`/`a104` byte-identical under both tiers; `a104`'s checker
   proves values across the full depth chain.
2. The three verbatim evidence rejections at pin `2016bf0` now
   mirror and lower (bindgen-unit-tested on those shapes), except
   `a105`'s if it lags — then its fail-loud text is pinned instead.
3. The §30 audits extend through recursion: every emitted array
   field is a collapsed pair and every accepted string-field
   position has a lowering, at any depth.
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.
