# R34 — the bytes of a value: `Context.bytesOf`, `bytesInto`, `fromBytes`

Status: **landed 2026-08-22** against `specs/blocks/stdlib.md` §18.
Origin: downstream request R34. Contract `391e7eb`, implementation
`ca5cb4e`.

## The request

R33 made the C layout of a schema class equal its WGSL layout. The
upload path takes `u8[]`. No construct yielded the bytes of a value
class or of a `FixedArray`, so the downstream generated one encoder
per schema over `Math.f32ToBits`.

## Findings on this host, at `ba6aa2e`

- `Context.bytesOf` failed with S014 "`Context.bytesOf` is outside
  the accepted Context subset (Q6/Q7/Q34)".
- Both tiers zero a fresh aggregate. C11 does not specify the padding
  bytes after a struct assignment or a by-value return *(docs)*;
  Apple clang 21 at `-O0` and `-O2` preserved them in one probe
  (`_Alignas(16)` `Vec3f` through a by-value return and a by-value
  parameter). The rule does not depend on that: the output zeroes
  every padding byte from the layout-derived ranges.

## What landed

`Context.bytesOf<T>(value): u8[]`, `Context.bytesInto<T>(value,
target, offset): void`, `Context.fromBytes<T>(bytes, offset): T`.
`T` is a `@CStruct` value class or a `FixedArray` whose storage
holds sized numerics, booleans, enums, and padding only; the checker
walks nested value classes and `FixedArray` elements and names the
offending field. Every boundary struct is rejected (the conservative
reading of §18.1 rule 2). A lone scalar is rejected. `bytesOf`
returns a `u8[]` of the layout size. `bytesInto` and `fromBytes`
trap with kind 1 when `offset + sizeof(T)` exceeds the array length
(64-bit compare, before any write). Padding bytes in the output are
zero on both tiers; `fromBytes` copies the storage, padding
included, and runs no constructor or initializer.

HIR: `Callee::ContextBytes { function, ty }` on `ExprKind::Call`.
Codegen: `Layouts::padding_ranges` (one source for both tiers); the
dev tier copies from the aggregate storage and zeroes the ranges;
the ship tier passes `(uint32_t)sizeof(T)` to the runtime and to
`memcpy`, then `memset`s the ranges. Runtime:
`subscript_rt_array_from_bytes` and `subscript_rt_array_byte_range`
with the peer null and liveness gates.

Corpus: `a142-bytes-of` (32-byte `FixedArray<Vec3f, 2>`; element
bytes `0,0,128,63,0,0,0,64,0,0,64,64,0,0,0,0`; `bytesInto` at
offset 4; `fromBytes` round-trip; byte-exact on both tiers),
`r138-bytes-of-reference-class`, `r139-bytes-of-string-element`,
`t51-bytes-into-range` ("byte range at offset 5 with size 16
exceeds array length 20", kind 1, both tiers).

## Gates (this host, at `ca5cb4e`)

- `cargo test --offline --workspace`: 55 suites, 973 passed, 0
  failed, 1 ignored, in both profiles.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden, `.expected`, header, and mirror
  byte-identical. Goldens 142; rejects 135; trap corpus 51.

## Review (fresh no-context subagent)

One MAJOR, fixed before the commit: the two new FFI entry points
dereferenced their pointers with no null or liveness gate, unlike
every peer array entry point. MINOR, fixed: the ship tier mixed the
layout size with `sizeof`; the top-level rejection message repeated
the type; an unreachable `IterResult` arm; two doc sentences. MINOR,
recorded and not fixed: the public free function `padding_ranges`
rebuilds the layouts per call and has no production consumer;
`Layouts::padding_ranges` takes the module as a second parameter.

## windows-msvc (measured at `9c6195d`)

- `cargo test --offline --workspace`: 57 suites, 967 passed, 0
  failed, 1 ignored. The 17 tests fewer than the clang host are the
  `offsetof` proof and the interop-fixture entries.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
  `cargo fmt --check`: exit 0. `tsc` 5.9.2 gate: exit 0.
- The golden sweep compared 93 entries and skipped 49. `a142` is in
  the compared set: dev-JIT, ship-C-AOT, and the golden agree byte
  for byte under MSVC `cl` 19.44.35222, `/std:c11 /O2`. Line 1 is
  `32`, so `FixedArray<Vec3f, 2>` keeps the 16-byte stride. Line 2
  ends `0,0,0,0`, so the four padding bytes of the `_Alignas(16)`
  `Vec3f` are zero. The C11 padding question in the findings above
  does not reach the output: the emitter zeroes the ranges.
- `t51` ran on both tiers. The trap suite excludes the interop
  probes only, and `t51` makes no foreign call.
