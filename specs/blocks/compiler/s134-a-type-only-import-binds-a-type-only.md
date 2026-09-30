<!-- §134 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 134. A type-only import binds a type only

*(Added 2026-09-30.)* Origin: found during §128 (a Phase Review
probe). The checker accepts programs that `tsc` rejects, which breaks
CLAUDE.md invariant 5.

Problem: the checker binds `import type { X }` and `import { type X }`
as ordinary imports, so `X` is usable as a value. Measured at
`95cb7785` through `subscript run`, with `lib.ts` exporting a class
`Box`, a function `mk`, a module global `g`, and an enum `E`:

- `import type { Box }` then `new Box()`: runs, prints `1`.
- `import type { mk }` then `mk()`: prints `2`.
- `import { type Box }` then `new Box()`: prints `1`.
- `import type { g }` then `` `${g}` ``: prints `3`.
- `import type { E }` then `E.A`: prints `1`.

`tsc` 5.9.2 rejects each with TS1361 ("'X' cannot be used as a value
because it was imported using 'import type'"). `import type { Box }`
with `const b: Box | null = null` is accepted by both.

### 134.1 Rules

1. A type-only import binding (`import type { … }`, or a specifier
   written `type X` in `import { … }`) binds its name in type positions
   only. The scope entry carries that fact, where the import registers
   it (as §127 carries the import fact).
2. A use of a type-only binding as a value (a call, `new`, a read, a
   member read such as `E.A` or a static member, a function value, an
   assignment target, a spread, a `Worker.spawn` entry) is S100 at the
   use, with a message that names the binding and says it was imported
   with `import type`. It is reported once per use site.
3. A type-only binding in a type position (an annotation, a type
   argument, a return type, `implements` if in the surface) resolves as
   today.
4. A type-only import of a name that has no type meaning (a function,
   a module global) is legal as an import; any use of it is rule 2 or
   the existing type-position diagnostic.
5. A re-export of a type-only binding (`export { X }` of a name bound by
   `import type`) follows §128 rule 7b: type-only export forms are
   outside the surface. The round measures what the checker does today
   and states it.

### 134.2 Acceptance

1. Red first: a reject entry (a directory entry) with `import type
   { Box }` then `new Box()`, accepted at `95cb7785`; the header states
   `tsc: rejects TS1361`.
2. Unit tests: each rule 2 use form, for both spellings, each exactly
   one diagnostic; controls: the same uses through an ordinary import
   are accepted; a type-only binding in each rule 3 position is
   accepted.
3. No existing `.expected` golden moves.
