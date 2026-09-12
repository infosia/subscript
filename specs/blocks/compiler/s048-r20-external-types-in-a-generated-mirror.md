<!-- §48 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 48. R20 — external types in a generated mirror

Owner decision 2026-08-03 (downstream request R20, non-blocking).
A host header that traffics in another mirror's handles cannot be
bound: `subscript bind` either fails at the boundary use site
(`unmapped C type ... at a boundary use site`) or, via a typedef,
emits a second declaration that collides
(`duplicate class name`) or an independent brand that will not
substitute. The language already accepts the shape — a declaration
referencing a type another ambient file declares type-checks and
runs on both tiers (downstream-measured) — so only the generator
is missing it.

The existing diagnostic names two remedies, a mapping and a
typedef, and **neither exists**. That is its own defect: a
fail-loud message must name a mechanism the tool has.

### 48.1 Rule

A C type may be declared **external** to a binding: bindgen
*references* it and does not declare it. An external type is
spelled in the header itself, so `subscript bind <header>` stays
reproducible from the header alone — the §12.2 byte-identical
regeneration gate must not depend on out-of-band arguments:

```c
/* @subscript-external SGPUTextureView */
```

The emitted mirror records the provenance
(`// @subscript-c-external type="SGPUTextureView"`), references the
name at every use site, and emits no declaration for it. Resolution
is the program's: the other mirror must be among its ambient files,
and if it is not, the existing unknown-type-name error fires at the
language level — fail loud, one layer down, not a silent brand.

An external name that the header never uses is an error (a
directive that does nothing is a mistake, and this is a
generated-header convention where silence hides generator bugs).
Declaring a type external *and* defining it in the same header is
an error for the same reason.

The `unmapped C type` diagnostic is rewritten to name this
mechanism instead of the two that do not exist.

### 48.2 Corpus and gates

The interop fixture gains a second small header whose boundary
declarations use a type `interop.h` declares, marked external;
`subscript bind` produces a mirror that references without
declaring; a corpus entry binds both mirrors in one program and
crosses the boundary with a value obtained through one and consumed
through the other, byte-identical under both tiers. Bindgen tests
pin: the emitted mirror contains no declaration of the external
name; an unused external directive is an error; external plus local
definition is an error; the regeneration byte-compare covers the new
header.

### 48.3 Exit criteria (pre-registered)

1. The new fixture header binds, and its mirror declares no
   external type while referencing it at every use site.
2. The two-mirror corpus entry is byte-identical under both tiers.
3. The three error cases pin (unused directive, external plus
   definition, unresolved at the language level).
4. The rewritten `unmapped C type` diagnostic names only mechanisms
   that exist, pinned by a test.
5. Regeneration byte-compare green for both headers; no existing
   golden moves; full gates green.
