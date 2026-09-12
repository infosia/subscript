<!-- §13 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 13. P6 — production-C-header interop (host-agnostic)

Owner decision 2026-07-24. P5 proved every interop *pattern* on a small
synthetic header; P6 makes the toolchain bind an **arbitrary production C
header** (of the scale and shape of a real graphics/OS C API — ~200
functions, ~100 structs, preprocessor, attributes, doc comments). The
language stays **host-agnostic** (invariant 4): no external API is named,
depended on, or committed; the reference sweep stays zero. Capability is
proven on a neutral synthetic fixture that reproduces every production-C
shape; pointing the tool at a specific real header is a **local,
uncommitted** step the tool supports for any header path.

Committed evidence names no external project. A real production header
may be bound locally to demonstrate the capability; that demonstration
is not committed and committed write-ups describe it generically (by
scale/shape, never by API name).

### 13.1 Real C parser (replaces the fixture parser)

The `bindgen` crate's narrow fixture parser (`cparse`) is replaced by a
**libclang-based frontend** (`clang-sys`, pinned; libclang resolved at
build/run time with a documented env override). It parses real C:
preprocessor (`#define`/`#if`/`#include`), function/nullable attributes,
doc comments, `typedef`, nested structs, function-pointer typedefs,
`static const` constants, enums, and flag typedefs. Gate: it regenerates
the existing `corpus/interop/interop.generated.d.ts` **byte-identically**
(proving the new frontend is a superset of the old), and parses a new
neutral fixture carrying the production-C features above.

### 13.2 New binding shapes

- **Descriptor-embedded `(count, pointer)` arrays.** Production headers
  spell arrays as adjacent fields *inside* a larger struct
  (`size_t <n>Count; const T* <n>;`), not as a standalone two-field
  descriptor. The mirror generator recognizes the embedded pair **only in
  the exact layout the lowering can reconstruct: the `size_t` count field
  immediately precedes the `const T*` pointer field (count-first,
  contiguous)**, and maps the pointer field to `T[]` with the count
  elided; zero-copy lowering as in §12 / a26 / a31. Any other spelling
  (pointer-first, or a non-adjacent count) is **not** recognized as an
  embedded array — the bare `const T*` field is then an unmapped boundary
  type and the mirror generator **fails loud** (never a silently
  wrong-marshaled mirror). The recognizer accepts exactly what both tiers
  fill; the mirror discards C field offsets, so it must not accept a
  layout the lowering cannot honor.
- **Flag typedefs.** `typedef <intN> XFlags;` + `static const XFlags
  X_A = …;` → a `uXX` alias plus `declare const` members, combinable with
  `|` (Q18), proven end-to-end (declare, combine, pass to a foreign call,
  observe).
- **Untyped bulk-data facade.** For a `void const* data, size_t size`
  (byte-size) API, the documented path is a thin typed C facade taking a
  typed slice descriptor (§ a31), bound as `T[]`, zero-copy, both tiers.
  A fixture proves the untyped API + facade + count→bytes conversion.

### 13.3 Async callback model

Production callbacks register a callback-info now and fire later
(host-driven), unlike P5's synchronous single fire (a28). P6 proves a
**deferred** fire: the host harness stores the registration and invokes
it after the registering call returns; the userdata-lifetime rule
(userdata must outlive the registration) is enforced or the misuse traps
with a stated diagnostic. Corpus entry + both-tier golden.

### 13.4 Scaled layout proof

The neutral fixture reaches production scale/complexity (intrusive
chains, by-value string views, nested structs, flag typedefs,
descriptor-embedded arrays, dozens of structs); the `offsetof` suite
(§12.3) asserts language layout == platform C compiler for **every**
mirrored struct at that scale.

### 13.5 Generic header CLI + local capability demo

`subscript-bindgen --header <path>` runs the libclang frontend on any
header and emits the mirror. *(Since 2026-07-30 this obligation is
discharged by `subscript bind` — same library entry, byte-identical
output, cli.md §10 — and the standalone binary is retired, cli.md
§11.)* The capability is demonstrated in-session
on a real production header (mirror produced, an `offsetof` spot-check
against the platform C compiler); this run is **not committed** and the
committed record describes it by scale/shape only. The committed proof
is the neutral fixture passing every gate.

### 13.6 Staging

- **P6.1** — libclang frontend replacing `cparse`; byte-identical
  regeneration of the existing mirror; neutral fixture with production-C
  features (macros/attributes/doc-comments/static-const/flag-typedef)
  parsed. Foundation.
- **P6.2** — the §13.2 shapes (descriptor-embedded arrays, flags,
  untyped-data facade): fixture + corpus + both-tier gate.
- **P6.3** — §13.3 async model, §13.4 scaled offsetof, §13.5 generic
  CLI + local real-header capability demo.

### 13.7 Gate

The neutral production-scale fixture binds end-to-end: the mirror
regenerates byte-identically, every mirrored struct's layout == the C
compiler's, the new shapes (embedded arrays, flags, facade, async) each
have a passing both-tier corpus entry with a committed golden, and the
generic CLI binds an arbitrary header path. Reference sweep clean (no
external-API names in tracked files); invariant 4 intact.
