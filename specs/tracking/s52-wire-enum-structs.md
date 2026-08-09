# §52 — wire-mapped aliases in boundary structs

Status: **landed 2026-08-09** against `specs/blocks/compiler.md`
§52. Origin: the §50.2/§51.2 parked item, owner-scheduled
2026-08-09 on the downstream shapes in
`specs/tracking/r24-bind-cenum.md`. Contract `ca9411c`,
implementation `c90ed0e`.

## The decision

A wire-mapped alias's `i32` discriminant is the declared wire value
itself; plain Q32 aliases keep declaration order. Two facts forced
it:

1. Invariant 1: a boundary struct's memory is the C layout, so an
   alias member slot must hold the wire value.
2. R7.2 array pairs are zero-copy, so alias elements in script
   arrays must be wire values — element conversion copies would
   abandon zero-copy.

Mirror `declare enum` members already work this way (the language
`Enum` value is the C value), so the revision aligned the alias
with landed practice. §50 carries the revision notes; every §50
observable stayed fixed.

## What landed

- Equality, `switch` labels, and formatting resolve by wire value,
  integer compares only. The §50 parameter-direction table is
  deleted (identity pass). Return crossings keep membership
  validation and the kind-24 trap.
- New boundary positions: direct boundary-struct members, boundary
  array-pair elements, mirror-class constructor parameters. Plain
  aliases stay rejected everywhere at the boundary.
- Validation sits where C data enters script: foreign returns and
  boundary-struct member reads. `t49` pins the member-read trap
  (`unknown wire value 12345 for CEnum alias \`SubWireMode\`` at
  the read position, both tiers identical). Locals, parameters,
  and descriptor-class fields do not re-validate.
- Bind lifts the struct-member and embedded-pair errors for
  annotated typedefs; the other §51.2 errors stand.
- `a131` proves the memory claim: C-side echoes of a constructed
  struct print the wire values (-7, 42, 23 — the last through the
  zero-copy embedded pair), and a C-filled struct reads back
  through a `switch`.

## Gates

`cargo test --offline --workspace --release` exit 0. `tsc` gate
exit 0. `subscript bind corpus/interop/wire-enum.h` reproduces the
extended mirror byte-identically (verified outside the test
suite). SHA-256 ledger: of 181 pre-implementation goldens and
generated mirrors, exactly one changed — `wire-enum.generated.d.ts`,
from the header extension — and the other 180 are byte-identical,
including every Q32 entry (`a91`, `a115`, `a118`, `a129`, `a130`,
`t48`). The representation revision is observably invisible, as
pre-registered.

## Out of scope, parked

Element-wise readback of alias arrays from C (parked with
recursive readback, on downstream evidence).
