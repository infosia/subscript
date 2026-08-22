# R33 — an alignment override on `@CStruct` value classes

Status: **landed 2026-08-22** against `specs/blocks/compiler.md`
§62. Origin: downstream request R33. Contract `e005d79`,
implementation `49bdd1d`.

## The request

The downstream uploads `FixedArray<T, N>` of `@CStruct` classes to
GPU buffers with no encoder. WGSL aligns `vec3<f32>` and `vec4<f32>`
to 16 and `vec2<f32>` to 8; C aligns three `f32` fields to 4. No
spelling raised the alignment of a value class. The downstream asked
for a class-level override, `@CStruct({ align: N })`.

## Findings on this host, at `4313dcf`

- The call form failed with S100 "the only decided decorators are
  the ambient `@CStruct` and `@Descriptor`"; the class was then not
  a value class.
- Two sites computed the class layout (`compiler/src/check/layout.rs`,
  `codegen/src/layout.rs`), both with alignment = max field
  alignment. The ship tier emitted the struct with no alignment
  attribute.
- Apple clang 21, C11, `_Alignas(16)` on the first field: `Vec3f`
  16/16, `Particle` 32/16, `Mixed` 32/16 with `p` at 16, `Mat3x3f`
  48/16, array stride 16, `Vec2f` with `_Alignas(8)` 8/8. These
  equal the downstream's table.
- Stock `tsc` 5.9.2 accepted the overload and rejected `align: 3`,
  an unknown key, and `@Descriptor({ align: 16 })`.
- Heap payloads start 16-aligned on both allocators (`HEADER_SIZE`
  16, allocation alignment 16).

## What landed

`@CStruct({ align: N })` with `N` in `{2, 4, 8, 16}` sets the class
alignment and rounds the size up to it; field offsets do not
change. `N` below the natural alignment is S100 with both numbers.
Any other key, a non-literal, a second argument, an empty literal,
and the call form on `@Descriptor` are S100. A generic template
carries the override into every instantiation. `ClassDef` gains
`alignment_override: Option<AlignmentOverride>`; both layout sites
take `max(natural, override)`; `emit_one_typedef` puts
`_Alignas(N)` on the first field declaration. The `offsetof` proof
adds `Vec3f`, `Mixed`, `Mat3x3f`, and `Vec2f` with the C structs
declared in the probe source; `interop.h` did not change. The
prelude gains the overload; the language reference gains one
sentence.

Corpus: `a141-cstruct-align` (nested after an `f32`, a
`FixedArray<Vec3f, 4>` field, copy-on-assign; byte-exact on both
tiers), `r135-cstruct-align-below-natural` ("requested alignment 2
is below the natural alignment 4 for `InvalidAlignment`"),
`r136-cstruct-align-not-in-set`, `r137-descriptor-align`.

## Red, at the contract pin

The probe and the three reject entries failed with the decorator
S100 above. `a141` was not accepted.

## Gates (this host, at `49bdd1d`)

- `cargo test --offline --workspace`: 55 suites, 967 passed, 0
  failed, 1 ignored, in both profiles.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden, `.expected`, header, and mirror
  byte-identical. New: a141's golden (141 total); rejects 133.

## Review (fresh no-context subagent)

No CRITICAL or MAJOR. MINOR, recorded and not fixed:

- A below-natural override on a generic template reports one S100
  per instantiation (same span, the instance name differs).
- `a141` prints values only. Its output does not change when the
  override is removed; the alignment numbers are pinned by the
  layout unit test, the C-emission test, and the `offsetof` proof.
- `@CStruct({})` reports "options must contain only the `align`
  key".

## windows-msvc

`_Alignas` under `cl /std:c11` is not measured on this host
*(docs)*. The next windows-msvc run confirms the cemit test
`aligned_value_class_emits_alignas_on_the_first_field` and the
a141 golden.
