<!-- §25 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 25. Q33 — literal-constructible descriptor classes

Owner decision 2026-07-31 (collisions.md Q33, C1/C7 exceptions;
downstream request R1). The decision text lives in Q33; this section
is the implementation contract.

### 25.1 Checker

`@Descriptor class` declares a data-only reference class: any
constructor, method, or `extends` clause is rejected; each member is
required (`name!: T`) or defaulted (`name?: T = expr`); `?` without
an initializer, or an initializer on a `!` member, is rejected. An
object literal type-checks only in a context expecting a
`@Descriptor` class, against exactly that class: missing required
members, excess members, and literals against unmarked classes are
rejected; nested literals and array-of-literal members check
recursively; Q32 alias members and defaults compose. `?` outside a
`@Descriptor` class keeps today's rejection. Codes: reuse the
existing paths (mismatch, closed-property) where they fit; the
corpus entries pin whatever fires.

### 25.2 Lowering (both tiers)

A constructing literal lowers to the class's ordinary allocation
followed by member stores — explicit members from the literal,
omitted members from their default expressions, evaluated at the
construction site. Defaults are per-construction (a fresh nested
descriptor default is a fresh allocation, not a shared instance).
Byte-identical behavior across tiers under the standing gate; no
runtime additions.

### 25.3 Corpus

`a92-descriptor-literals` (accept): a descriptor with required,
defaulted, Q32-alias, nested-descriptor, and array-of-descriptor
members; constructions covering full literals, omission taking
defaults, `{}` against an all-defaulted descriptor, nesting, and an
argument-position call — golden prints prove filled defaults and
overrides under both tiers. Reject: `r90` missing required member,
`r91` excess member, `r92` literal against an unmarked class
(`tsc`-clean — stock TS accepts it structurally; the
strictly-narrower proof), `r93` `?` member without initializer in a
descriptor, `r94` a method in a descriptor class, `r95` `new` on a
descriptor class (`tsc`-clean; added at landing — literal
construction is the only construction).

### 25.3a Amendment — literals through `Descriptor | null` (2026-08-02, R17)

Downstream bug report, reproduced at the pin: contextual typing for
descriptor object literals stopped at `Type::Class` and never
unwrapped `Nullable`, so a literal whose contextual type is
`DescriptorClass | null` — a defaulted member
(`m?: D | null = null`), a required member (`m!: D | null`), a
parameter, or the same positions under nesting and array
elements — rejected with a generic S100 while `null` and a typed
temporary were accepted. The shape is real and generated: §33's
`[nullable]` struct-pointer members mirror as exactly these
member types.

Rule: **in a contextual position typed `D | null` where `D` is a
descriptor class, an object literal takes the descriptor arm and
`null` keeps its meaning** — at any depth, matching the
non-nullable behavior of §25 otherwise (defaults, required-member
enforcement, excess rejection). A literal against
`PlainClass | null` keeps S005 (nominal rejection), pinned.

Corpus: `a117-descriptor-literal-nullable-member` (accept,
Red-first — the entry reproduces the rejection at the pre-fix pin
and lands with the fix): both member kinds, `{ m: {} }`,
`{ m: null }`, omission taking the `null` default, and the
array-element nesting from the downstream's controls; observation
`tsc`-strict-compatible (presence checks in template position for
the `?`-declared member, full narrowed reads on the
required-nullable member). `r116-object-literal-nullable-class`
(reject): `{}` against `PlainClass | null` pins S005 in the
nullable position (`tsc` status probed and recorded).

### 25.4 Exit criteria (pre-registered)

1. `a92` runs byte-identical under both tiers; the golden shows a
   default-filled value, an overridden value, and a nested default.
2. `r90`–`r94` pin (code, line); `r92` type-checks under stock `tsc`
   (verified standalone, recorded in its header).
3. The prelude declares `Descriptor`; the `tsc` gate is green with
   `a92` in the include set.
4. No existing golden moves; full gate green; the zero-warning sweep
   is unaffected.
5. Checker unit tests: required-present, default-filled,
   excess-rejected, unmarked-class-rejected, and the two member-form
   rejections.
