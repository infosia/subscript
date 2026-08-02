# §43 — absence-capable Q32-alias descriptor members

Status: **landed and verified 2026-08-02** against `compiler.md`
§43. Origin: downstream R16 (re-sent after the downstream
overwrote its own first send; not blocking, but its E2 shipped
WebGPU's `compare` member required, which made every explicit
sampler descriptor a comparison sampler and left the ordinary
sampler reachable only through an all-defaults helper).

Scope taken at the downstream's offered minimum — Q32-alias
members only. Grounding before contracting (`--lib es2022`
standalone, exit 0 each): the declaration form, presence tests with
narrowed reads, template prints, **and** the two shapes §43 rejects
(explicit `undefined` member value; unnarrowed read in a template)
are all stock-`tsc`-clean, so both rejects are strictly-narrower
pins. The `!== undefined` spelling was chosen because it is the one
form `tsc` itself narrows — the carve-out costs nothing in `tsc`
compatibility.

## §43.4 evidence (reviewer-run)

1. `a118-absence-capable-member` byte-identical under both tiers:
   present / absent-with-other-members / `{}` constructions, each
   observed through both narrowing arms.
2. `r117-explicit-undefined-member` (S012) and
   `r118-unnarrowed-absence-read` (S100) pin code and line;
   implementer `tsc` probes exit 0 for both, recorded in headers.
   `r93` unchanged (S012 at line 9) — it now pins the non-alias
   boundary.
3. Reviewer live probes at the landing: the WebGPU sampler shape
   prints `comparison:less` / `ordinary` / `ordinary` for
   `{compare:"less"}`, `{maxAnisotropy:16}`, `{}` — the three
   states the downstream could not previously express. **C7
   boundary verified intact**: `x !== undefined` on a
   non-absence-capable value rejects with a diagnostic naming the
   carve-out, and bare `undefined` keeps the original S012 text.
4. Gate 48 harnesses, 851 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; 118 goldens byte-identical across both tiers; no
   existing golden moved; zero-warning (119 accept sources) and
   generated-docs gates green.

## Implementer decisions recorded

Reserved discriminant is `-1` (ordinary alias members keep
`0..N`); presence tests erase the `undefined` token to an integer
comparison against `-1`, never touching the §24 formatting table;
narrowing reuses the path-based null-narrowing machinery including
its assignment invalidation. Defaulted omissions are unchanged;
absence-capable omissions materialize the sentinel. No bindgen
change — the C-side sentinel write stays the downstream
generator's, as the request scoped it.
