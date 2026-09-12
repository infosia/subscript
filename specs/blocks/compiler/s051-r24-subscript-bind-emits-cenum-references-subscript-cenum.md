<!-- §51 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 51. R24 — `subscript bind` emits CEnum references (`@subscript-cenum`)

Owner-scheduled 2026-08-09 (downstream request R24, follow-on to
§50). §50 landed the alias but no generated mirror can carry it:
the downstream's mirrors are bind output under the byte-identical
regeneration gate, so a bound signature's type comes from the C
type alone. This section gives bind a header directive that maps a
C spelling to an alias reference. The R20 external mechanism (§48)
is the model: bind references a name it does not declare.

### 51.1 Directive

A standalone header comment, two identifiers, in the
`@subscript-external` spelling family:

```c
/* @subscript-cenum EngineFrameFormat GPUTextureFormat */
```

The first identifier names a typedef the header declares. Its base
type is `int32_t`, or it is an enum typedef. The second identifier
is the alias name the mirror will reference; it must be a legal
type identifier.

Bind then emits the alias name at every use of the typedef in a
**direct parameter or return position** of a bound function. Bind
emits a provenance comment
(`// @subscript-c-cenum typedef="EngineFrameFormat" alias="GPUTextureFormat"`)
and **no declaration** of the alias. For an annotated enum typedef,
bind emits no `declare enum` for it. The wire table stays the
downstream generator's knowledge: bind never sees the
string-to-value correspondence and never checks it.

Regeneration stays `subscript bind <header>` with no extra
arguments (§48.1 reproducibility rule).

### 51.2 Loud bind errors

Each case is a bind error that names the site:

- The named typedef does not exist in the header.
- The typedef's base type is not `int32_t` and not an enum.
- The header never uses the typedef in a direct parameter or
  return position (a directive that does nothing is a mistake).
- The typedef is used anywhere else: a struct member, a pointer
  target, an array element, or another typedef's base. Struct
  members wait for their own slice (§50.2 parked item; the
  downstream supplied shapes 2026-08-09, recorded in tracking).
- The alias name collides with any name the header declares or
  bind emits.
- A duplicate directive for the same typedef.

### 51.3 Resolution rule (the downstream's question)

The alias declaration must be an **ambient** declaration among the
program's ambient files. A module-scoped alias does not reach a
mirror. If the name does not resolve, the existing
unknown-type-name error fires at the language level (§48.1
precedent). If it resolves to a string alias without a wire
mapping, the §50 plain-alias boundary rejection fires. Bind
verifies neither: resolution is the program's, one layer down,
fail-loud.

### 51.4 Fixture and corpus

The §50.4 hand-authored mirror retires: `wire-enum.h` gains a
`typedef int32_t` spelling, the directive, and the generated
mirror joins the byte-identical regeneration gate. The alias
declaration moves to a hand-authored ambient file beside it.
`a129` and `t48` keep their goldens unchanged.

The header also gains an annotated **enum typedef** with explicit
values that match the alias's wire values, plus functions that use
it at parameter and return positions. `a130-interop-wire-enum-bind`
(accept) exercises that flavor end-to-end: a C return received, a
member passed back through a C echo, byte-identical under both
tiers.

Bindgen unit tests pin every §51.2 error case and pin that the
emitted mirror contains the provenance comment and no alias
declaration.

### 51.5 Exit criteria (pre-registered)

1. `subscript bind` reproduces both generated mirrors
   byte-identically from their headers alone; the hand-authored
   mirror is gone; `a129` and `t48` goldens do not move.
2. `a130` runs byte-identical under both tiers through the
   annotated enum typedef.
3. Every §51.2 error case pins in a bindgen unit test, each naming
   the site.
4. A checker unit test pins the unresolved-alias error for a mirror
   that references an alias no ambient file declares.
5. Full gate, `tsc` gate, and the zero-warning sweep are green; no
   existing golden moves.
