# Q33 — literal-constructible descriptor classes: evidence

Status: **landed and verified 2026-07-31** against `compiler.md` §25
and `collisions.md` Q33 (C1/C7 exceptions). Origin: the downstream
WebGPU binding project's request (HANDOFF/REPORT exchange, R1),
designed as the follow-on to Q32 so one C7 revision cycle covered
both. Contract committed first; the `tsc`-acceptance of the whole
mechanism (`@Descriptor` + `name!: T` + `name?: T = expr` + literal
construction, nested and `{}`) was probed against stock `tsc` before
contracting — the `!` spelling is imposed by `strict`, recorded in
Q33 so it is not "simplified" away later.

## §25.4 evidence (reviewer-run)

1. `a92-descriptor-literals` byte-identical under both tiers; the
   golden shows default-filled (`defaulted=2,64,safe`), overridden
   (`full=1,128,fast,9`), all-defaulted `{}`
   (`all-defaulted=true,ready`), nested-default, array, and
   argument-position constructions.
2. Reject pins: `r90` missing required (S100:13), `r91` excess
   member (S004:13), `r92` literal against an unmarked class
   (S005:13, `tsc`-clean standalone — the strictly-narrower proof),
   `r93` `?` without initializer (S012:9), `r94` method (S100:10),
   `r95` `new` on a descriptor (S100:14, `tsc`-clean; added at
   landing to pin the implementer's adopted resolution).
3. Prelude declares `Descriptor`; `tsc` gate exit 0 with `a92`
   included.
4. No existing golden moved (91 prior hashes unchanged); gate 48
   harnesses, 736 passed, exit 0, read directly; zero-warning sweep
   unaffected.
5. Per-construction default freshness pinned by
   `descriptor_nested_defaults_are_fresh_per_construction` (distinct
   nested allocations, both tiers).

## Implementer decisions recorded (adopted into Q33/§25 at landing)

`new` on a `@Descriptor` class is rejected — literal construction is
the only construction, so required members cannot be left
uninitialized (`r95`). Member expressions and defaults evaluate in
declaration order. No new S-codes; the existing mismatch,
closed-property, and optional-property paths carry the rejections.
