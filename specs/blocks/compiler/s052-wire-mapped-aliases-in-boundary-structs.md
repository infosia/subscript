<!-- §52 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 52. Wire-mapped aliases in boundary structs

Owner-scheduled 2026-08-09, on the downstream shapes recorded in
`specs/tracking/r24-bind-cenum.md` (33 C enums, most reaching
scripts as descriptor members; the §50.2/§51.2 parked item). This
section lands the parked item and revises one §50 sentence.

### 52.1 Representation revision — the discriminant is the wire value

For a wire-mapped alias, the `i32` discriminant **is** the declared
wire value. Plain Q32 aliases keep the declaration-order
discriminant unchanged.

The reason is invariant 1: a boundary struct's memory is the C
layout, so an alias member slot must hold the wire value; an
array-pair of alias elements is zero-copy only if the elements are
wire values. (Mirror `declare enum` members already work this way:
the language `Enum` value is the C value.) One representation for
every position removes every conversion except validation where C
data enters script.

Consequences, each observably invisible:

- §24 equality and §41 `switch` stay integer compares; case labels
  and member-literal constants lower to wire values.
- §24 formatting resolves the string-table entry **by wire value**,
  never by string comparison. Every value that reaches a formatting
  site was validated where it entered, so a lookup miss is
  unreachable.
- §43.2 absence keeps its rule: the sentinel is a reserved value
  outside the member set (now outside the wire-value set), selected
  by the implementation, never observable.
- §50.3 collapses as noted there: parameter direction is identity;
  return direction keeps membership validation plus the kind-24
  trap.
- Every existing golden is byte-identical across this revision —
  an exit criterion, not an expectation.

### 52.2 Checker

A wire-mapped alias is legal in these boundary positions, beyond
§50.2's parameter and return:

- a direct member of a boundary struct;
- the element of a boundary array-pair member (§12 standalone
  descriptor and §13.2 embedded count-first pair) — the
  `viewFormats` shape;
- a constructor parameter of a mirror class.

A plain Q32 alias stays rejected in every boundary position.

Direction scope: script builds and writes these structs (the
descriptor-write path) and reads direct members back. Element-wise
readback of alias **arrays** from C stays out of scope, parked with
recursive readback.

### 52.3 Lowering (both tiers)

- Construction and member writes are plain stores: the value
  already is the wire value. Array-pair members keep the existing
  zero-copy lowering.
- A read of an alias member from a boundary struct validates
  membership and traps on an unknown value (kind 24, the §50.3
  diagnostic, at the read position). Boundary-struct slots are the
  one place C can write an alias value; validated reads keep every
  alias-typed variable a member value, which is what makes the
  formatting lookup miss unreachable.
- Reads of alias locals, parameters, and descriptor-class fields do
  not validate: their producing sites already did.
- Both tiers byte-exact under the standing gate; no string
  operation at any alias site.

### 52.4 Bind

For a §51-annotated typedef, the struct-member and embedded-pair
uses stop being errors: a direct struct member emits the alias
name; a recognized array-pair whose element is the annotated
typedef emits `Alias[]`. Every other §51.2 error case stands
(pointer target outside a recognized pair, array element outside a
pair, typedef base, collisions, zero uses, duplicates).

### 52.5 Corpus and fixture

`wire-enum.h` gains a struct carrying a direct wire-alias member
(each flavor), an embedded count-first alias pair, and plain
scalars, plus C functions that receive the struct (echo the wire
values) and fill a caller struct for readback. The generated
mirror re-joins the regeneration gate.

- `a131-interop-wire-enum-struct` (accept): script constructs the
  struct, C echoes the member and element wire values; C fills a
  struct and script reads the alias member back through a `switch`;
  byte-identical under both tiers; no converter in script.
- `t49-wire-enum-struct-unknown-member` (trap): C fills the member
  with a value outside the mapping; the script read traps; one
  identical diagnostic under both tiers.
- Boundary rejections stay checker unit tests (the §50 precedent):
  plain alias as struct member, as pair element, as constructor
  parameter.

### 52.6 Exit criteria (pre-registered)

1. `a131` runs byte-identical under both tiers; the echoed values
   prove the struct memory holds wire values.
2. `t49` traps with one identical diagnostic under both tiers,
   naming the alias and the value, at the member read.
3. **No existing golden moves** — including `a91`, `a115`, `a118`,
   `a129`, `a130`, `t48`: the representation revision is observably
   invisible.
4. `cemit` unit tests updated and added: switch case labels are
   wire values; the §50 parameter-table access is gone (identity);
   a member read emits validation plus the trap path; no string
   operation at any alias site.
5. Checker unit tests: the three new positions accepted for
   wire-mapped aliases and rejected for plain aliases.
6. Bindgen tests: struct-member and embedded-pair emission for an
   annotated typedef; the remaining §51.2 error cases retained;
   byte-identical regeneration of the extended header.
7. Full gate, `tsc` gate, and the zero-warning sweep are green.
