# §41 — `switch` over Q32 literal-union aliases: evidence

Status: **landed and verified 2026-08-02** against `compiler.md`
§41. Origin: downstream request R14 (not hard-blocking; taken over
the `if/else` fallback because the next slice's `GPUTextureFormat`
has 102 members on the `createTexture` path — linear compares plus
silent non-exhaustiveness were not worth shipping).

Grounding recorded before contracting: exhaustive, `default`-subset,
missing-member, and duplicate-member alias switches are all
stock-`tsc`-clean (`--lib es2022` standalone), so r112 and r114 are
strictly-narrower pins; a non-member label is `tsc`-rejected
(TS2678), recorded in r113. Standalone probes must pass
`--lib es2022` — without it lib.dom's `Worker` collides with the
Q35 prelude `Worker`.

## §41.4 evidence (reviewer-run)

1. `a115-switch-literal-union` byte-identical under both tiers
   (exhaustive three-member dispatch plus the `default`-subset
   variant).
2. `r112`–`r114` pin code and line with `tsc` statuses recorded
   (implementer probes: r112 exit 0, r113 exit 2/TS2678, r114
   exit 0).
3. The cemit test pins a115's ship emission: C `switch` with `i32`
   case labels, no string-comparison call.
4. Reviewer live probes at the landing: the downstream's
   `GPUBufferMapState` shape dispatches 10/20/30 under `subscript
   run`; a missing-member switch rejects with a diagnostic that
   **names the missing labels** ("missing case labels: \"unmapped\",
   \"pending\"") — the generator-facing guarantee R14 asked for.
5. Gate 48 harnesses, 834 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; differential sweep 115 entries; zero-warning and
   generated-docs gates green; only a115's golden is new.

## Implementer decisions recorded

Alias case labels must be syntactic member string literals; invalid
or duplicate labels suppress the secondary missing-member
diagnostic; missing-member checking runs only without `default`
while duplicates always reject; only `StringAlias` ship switches
changed emission (C `switch` over the discriminant) — enum,
integer, and string switches are untouched.
