<!-- §126 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 126. A renamed import binds the imported declaration

*(Added 2026-09-29.)* Origin: the §125 Phase Review found it; the owner
chose to fix it on 2026-09-29.

Problem: `resolve_imports` (`compiler/src/check/signatures.rs`) looks
up the local name of an import specifier in the target module, not the
imported name. Measured at `1295cb4` through `subscript run`, with
`lib.ts` exporting `f` (returns 1) and `g` (returns 2):

- `import { g as f } from "./lib"` and `print(`${f()}`)`: the checker
  accepts and the program prints `1`. `node` prints `2`. This breaks
  C14: this compiler accepts a program and gives it a different value.
- `import { f as h } from "./lib"`: S016 "`h` is not exported by
  `./lib`". `tsc` accepts it.

After §125, a renamed import is the one way a module that declares a
name reaches another module's declaration of that name.

### 126.1 Rules

1. An import specifier `{ a as b }` resolves `a` in the target module's
   exports and binds `b` in the importing module. `{ a }` is
   `{ a as a }`.
2. An S016 for an import names the imported name (`a`), at the
   position of that name.
3. A renamed import of every declaration kind a module exports binds
   the same declaration as the unrenamed import: a function, a class,
   a module global (a live binding to the one storage), an enum, a
   string alias, a generic function, and a generic class.
4. Two imports that bind one local name in one module are S017, as
   today (§82.2).
5. A poisoned import (§63 rule 5) keeps its current record of both
   names.

### 126.2 Acceptance

1. Red first: an accept entry (a directory entry, `js-comparable`, all
   three engines and `node`) with `import { g as f }` where the module
   also exports `f`, and renamed imports of every rule 3 kind. At
   `1295cb4` the entry fails: a renamed import of a name the module
   does not also export is S016, and the `g as f` shape alone prints
   the wrong value. The round records both Red outputs.
2. A reject entry: a renamed import of a name the target does not
   export, S016 at the imported name. Its header states what `tsc`
   does, measured.
3. No existing `.expected` golden moves.
