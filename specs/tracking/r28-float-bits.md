# R28 — binary32 bit access on `Math`

Status: **landed 2026-08-15** against `specs/blocks/stdlib.md` §17.
Origin: downstream request R28. Contract `586cfd1`, implementation
`fc54acb`.

## The request

The downstream generates encoders for GPU records that interleave
`u32` and `f32` members. A script encodes each integer width with
shifts and masks, and encodes no float at all. The downstream
refuses an IEEE-754 encoder in script arithmetic: subnormal,
infinity, NaN, and round-to-nearest-even branches, with no reference
to check against. Ask: bit access to binary32, in both directions.

## Findings on this host, at `2029350`

- The report is correct. `runtime/src/math.rs` holds `clz32`,
  `imul`, and `fround`. No bit-access surface exists in `*.rs` or
  `*.ts`.
- Every `Math` member lowers in both tiers to one shared runtime
  symbol (stdlib.md §0.2). An added member agrees across the tiers
  by construction.
- The prelude already merges members into lib interfaces (`Map`,
  `Set`, `RegExp`), so a `Math` merge keeps the `tsc` gate clean.

## What landed

`Math.f32ToBits(value: f64): u32` narrows with the `fround` rule
and returns the bit pattern; a NaN result is canonical
`0x7FC00000`, because casts give NaN an unspecified payload.
`Math.f32FromBits(bits: u32): f64` widens exactly. Laws and change
sites: stdlib.md §17. `MathFn` gains `symbol()`; the 35 existing
symbols do not change.

Corpus: `a135-f32-bits` (accept; the golden pins 1.0, the `fround`
agreement law on 1.1, `-0`, infinity, overflow, canonical NaN, the
`2^-149` subnormal, and both round trips) and
`r127-f32-frombits-f64-arg` (S007).

## Red, at the contract pin

The checker rejected both members with S014, "outside the accepted
Math subset (Q19)": ten errors in `a135`, one in `r127`.

## Gates (this host, at `fc54acb`)

- `cargo test --offline --workspace`: 55 suites, 940 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden and `.expected` file is byte-identical;
  the only new golden is a135's (135 total).
