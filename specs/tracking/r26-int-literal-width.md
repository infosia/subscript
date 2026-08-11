# R26 — integer literals read at the target's width

Status: **landed 2026-08-11** against `specs/blocks/compiler.md`
§56. Origin: downstream request R26. Contract `11874ac`,
implementation `6f812b1`.

## The request

A `u64` literal above 9007199254740991 (2^53 − 1) fails with
S008, and the value is a valid `u64`. WebGPU types buffer sizes,
offsets, and copy sizes as `u64`. A downstream test computes the
constant instead of writing it.

## Findings on this host, before the contract

- `check_num_lit` reads the parser's `f64` view and range-checks
  through `f64`. The spelling (`raw`) is available and exact.
- The cap was the C3 decision, an open item marked "revisit with
  evidence" (`collisions.md` §3). R26 is that evidence.
- Stock `tsc` accepts the three boundary shapes (measured: exit
  0). TS 80008 is a suggestion, not an error. Invariant 5 holds.
- Both tiers carry the bits correctly today. Two adjacent
  defects: the C emitter spells `i64::MIN` as invalid C, and the
  literal shift-amount checks compare the bits as `i64`, so a
  `u64` amount with a negative bit pattern passes them.
- The ambient mirror channel (`int_literal_value`) also reads
  through `f64`; `bindgen` guarded it with a fail-loud error for
  flag values above 2^53 − 1 (`p5-interop.md` MINOR m2).

## What landed

The checker reads an integer literal from its source spelling
(decimal, `0x`, `0b`, `0o`, `_` separators) at the target's
width. The full `u64` and `i64` ranges compile. The HIR stores
the two's-complement bits; both literal shift-amount checks read
the bits at the operand's type. The C emitter spells `i64::MIN`
as `(-9223372036854775807ll - 1)`, the `i32::MIN` treatment. The
mirror flag channel reads at `u64` width, and `bindgen` drops its
guard; its guard test becomes an acceptance test. Synthesized
nodes without a spelling keep the pre-R26 path.

Corpus: `a132-int-literal-64bit` (accept, with golden; the `u64`
maximum in three spellings, 2^53 + 1, the `i64` maximum and
minimum), `r124-u64-literal-overflow`, `r125-i64-literal-underflow`
(both S008). C3 in `collisions.md` loses the cap and its open
item.

## Red, at the contract pin

`a132` before the implementation: rejected with six S008
diagnostics, one per declaration, for example
`integer literal 18446744073709551615 out of range for `u64``.

## Gates (this host, at `6f812b1`)

- `cargo test --offline --workspace`: 931 passed, 0 failed, 1
  ignored, exit 0. The same counts in the release profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden and `.expected` file is
  byte-identical; the only new golden is `a132`'s.
- The two handoff probes (`u64` maximum, `0xFFFFFFFFFFFFFFFF`,
  `i64` minimum) check clean.
