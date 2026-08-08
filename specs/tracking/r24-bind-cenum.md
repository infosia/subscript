# R24 — `subscript bind` emits CEnum references

Status: **landed 2026-08-09** against `specs/blocks/compiler.md` §51.
Origin: downstream request R24, follow-on to R23. Contract `300b62c`,
implementation `688a7a7`.

## The request

R23 landed the `CEnum` alias, but the downstream's mirrors are bind
output under the byte-identical regeneration gate, so no generated
mirror can carry the alias until bind maps a C spelling to it. R24
asked for a header annotation in the R20 `@subscript-external`
family, and answered the parked struct-member item with real shapes
(33 C enums; most reach scripts as descriptor members).

## The decision

The directive is a standalone two-identifier comment:

    typedef int32_t EngineFrameFormat;
    /* @subscript-cenum EngineFrameFormat GPUTextureFormat */

The downstream proposed a trailing comment on the typedef. The
standalone form was selected: `@subscript-external` (§48) is a
standalone self-naming directive, positional comment attachment has
no precedent in the frontend, and the downstream stated it can
produce any accepted shape. Both `int32_t` typedefs and enum
typedefs are accepted; every use must be a direct parameter or
return position this slice.

## What landed

- Bind emits the alias name at direct parameter and return
  positions, a `// @subscript-c-cenum typedef="…" alias="…"`
  provenance record, and no alias declaration. An annotated enum
  typedef emits no `declare enum`.
- Ten loud bind errors, each naming the site: missing typedef, base
  neither `int32_t` nor enum, zero direct uses, struct-member use,
  pointer-target use, array-element use, typedef-base use, alias
  collision with a header declaration, alias collision with a
  bind-emitted name, duplicate directive.
- Resolution rule (the downstream's contract question): the alias
  declaration must be ambient. An unresolved reference fails with
  the existing S100 `unknown type name` error, unit-tested.
- The R23 hand-authored mirror retired: `wire-enum.h` carries both
  directive flavors, the generated mirror joined the regeneration
  gate, and the wire tables moved to hand-authored
  `corpus/interop/wire-enum-aliases.d.ts`. `a129`/`t48` sources and
  goldens are byte-identical across the change. `a130` proves the
  enum-typedef flavor end-to-end.

## Findings

1. **The first implementation round stopped on a real gap.** The
   contract's provenance record requires a `cenum` case in the
   compiler's provenance parser (`compiler/src/provenance.rs`),
   which the handoff had fenced off. The coding agent stopped and
   reported instead of exceeding scope; the fix was authorized as a
   one-file scoped change. Rule kept: a mirror-format change
   touches the provenance parser; scope it in from the start.
2. The first fix round left the golden-capture utility loading the
   deleted hand-authored mirror (`codegen/src/bin/capture.rs`).
   Found by the agent's own report, fixed in a second scoped round;
   capture now reproduces the `a129` golden byte-identically.

## Gates

`cargo test --offline --workspace --release` exit 0. `tsc` gate
exit 0. `subscript bind corpus/interop/wire-enum.h` reproduces the
committed mirror byte-identically (verified independently of the
test suite). All 181 pre-implementation goldens, generated mirrors,
and the frozen `a129`/`t48` sources unchanged (SHA-256 ledger).

## Parked, now evidenced

Wire-mapped aliases as boundary-struct members. The downstream
supplied the real shapes 2026-08-09 (`SGPUColorTargetState`,
`SGPURenderPassColorAttachment`; 33 enums, mostly descriptor
members, 45% of its API layer in converters). The `@subscript-cenum`
directive already covers the header side; the checker and lowering
work is the open slice, next in line on owner scheduling.
