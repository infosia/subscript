# R23 — wire-mapped literal-union aliases across the bind boundary

Status: **landed 2026-08-09** against `specs/blocks/compiler.md` §50.
Origin: downstream request R23. Contract `3097472`, implementation
`d3657c2`.

## The request

Q32 barred literal-union aliases from boundary signatures (v1), so
the downstream lowered every boundary enum to a bare integer. Its
evidence: 45% of its generated API layer (1976 of 4346 lines) was
converter functions between union strings and mirror integers, and
its windowed example received a surface format as a typeless `i32`.
R23 asked for a declaration form that binds a union to an integer
wire representation, member by member, legal in bound signatures at
parameter and return positions.

## The decision

Owner decision 2026-08-08: the `CEnum` prelude generic.

    type CEnum<M extends Record<string, number>> = Extract<keyof M, string>;
    type A = CEnum<{ "m0": 0x10, "m1": 23, "m2": -7 }>;

Stock `tsc` resolves the alias to the string-literal union. Measured
2026-08-08 before the decision: the positive probe exits 0; a
non-member literal fails TS2322; a duplicate key fails TS2300. The
annotation-comment alternative was rejected: it puts the values
outside the type system and costs one comment line per member.

## What landed

- The alias is a Q32 alias in full; the in-language representation
  is unchanged. The wire value is not script-visible.
- Boundary: legal at parameter and return positions of bound
  functions, C type `int32_t`. The parameter crossing indexes a
  per-alias static table. The return crossing maps wire to
  discriminant; an unknown value traps (kind 24,
  `wire-enum-unknown-value`) with the alias name and the value,
  byte-exact across tiers. No string operation at the crossing.
- Checker rejections beyond stock `tsc` (all S100, corpus-pinned):
  fractional wire value, duplicate wire value, value outside `i32`,
  empty member set. A plain Q32 alias stays rejected at the
  boundary. Wire aliases inside composite boundary types stay
  rejected.
- Corpus: `a129` (non-dense hex/gap/negative wire values; C return
  received and switched on; members passed to a C echo), `t48`
  (unknown wire value traps identically under both tiers),
  `r121`–`r123` (each stock-`tsc`-clean; the strictly-narrower
  proof). `corpus/interop/wire-enum-tsc-gate.ts` imports the three
  rejects into the standing `tsc` project, so criterion §50.5-3 is
  machine-checked on every gate run, not verified by hand.

## Gates

`cargo test --offline --workspace --release` exit 0. `tsc` gate
exit 0. All 179 pre-existing goldens and generated mirrors are
unchanged (SHA-256 ledger taken before the implementation).
`bindgen/` and `specs/` untouched by the implementation.

## Out of scope, parked on downstream evidence

Wire-mapped aliases as boundary-struct members. Emission of the
form by `subscript bind` (the fixture mirror is hand-authored for
this reason). The return crossing is a linear scan over the wire
table; at the measured boundary scale (R22: ~1 ns per call) this is
acceptable for the observed use (one format query), and a dense or
binary mapping is a measured follow-up if a hot return-position
enum appears.
