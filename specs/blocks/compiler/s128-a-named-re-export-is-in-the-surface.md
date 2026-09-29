<!-- §128 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 128. A named re-export is in the surface

*(Added 2026-09-29.)* Origin: the host API module design (§129). The
owner put named re-exports in the surface on 2026-09-29.

Problem: a module cannot export a name that it imports or that another
module declares. `export { update as physicsUpdate } from "./physics"`
and `export { local as other }` are S100 ("only `export` declarations
and named imports are in the decided surface",
`compiler/src/check/declarations.rs`). The program loader follows only
import declarations (`parse_import_specifiers`,
`cli/src/program_loader.rs`), so a module that a re-export names is not
loaded. So a module that exposes names from its submodules needs a
wrapper function per name, and a wrapper cannot re-export a class, an
enum, a string alias, or a module global.

Measured with `tsc` 5.9.2 (`--strict --noEmit --target es2022 --module
esnext --moduleResolution bundler`) and `node`:

| Form | `tsc` | `node` |
| --- | --- | --- |
| `export { a, b as c } from "./m"` | accepts | runs |
| `export { a as b }` of a local declaration | accepts | runs |
| `import { a as l } from "./m"; export { l as b }` | accepts | — |
| A re-export of a re-export | accepts | — |
| `export { nope } from "./m"` (not exported) | TS2305 | — |
| Two modules that re-export `x` from each other, no declaration | TS2303 | — |
| Two modules that declare `a` and `b` and re-export each other's | accepts | — |
| Two `export *` of distinct declarations named `x` | TS2308 | — |
| Two re-exports that give one export name | TS2300 | — |
| A declaration and a re-export that give one export name | TS2323, TS2484 | — |
| `export * from "./m"` alone | accepts | — |

`node` printed `physics 1`, `audio 2`, `local` for a program with
`export { update as physicsUpdate } from`, `export { update as
audioUpdate } from`, and `export { local as localEntry }`.

### 128.1 Rules

1. `export { a as b } from "./m"` adds `b` to this module's exports.
   It binds no local name in this module, as TypeScript gives. `b`
   resolves to the declaration that `a` resolves to in `./m`, through
   any number of re-exports. `{ a }` is `{ a as a }`.
2. `export { a as b }` without `from` exports a top-level binding of
   this module under the name `b`: a declaration, or a named import
   (§126). The local name stays bound.
3. Every kind of declaration that a module can export can be
   re-exported: a function, a class, a module global (a live binding
   to the one storage, §125 rule 1), an enum, a string alias, a generic
   function, and a generic class. A re-exported name has the identity
   of its declaration (§125 rule 2); a re-export makes no copy.
4. Export resolution follows the dependencies between export names,
   not the file order. A chain of re-exports that never reaches a
   declaration (a cycle of aliases) is one S016 (rule 5b), and the
   message names every re-export in the cycle. A cycle in the module graph
   whose exports reach declarations is not an error.
5. A re-export of a name the source module does not export is S016 at
   that name, as an import is (§126 rule 2). A failed export name is
   poisoned in its module's export table: a re-export or an import of
   it reports nothing more. Each failure is reported once, at the site
   that fails.
5b. *(Added 2026-09-29 after the second Phase Review: two reviews found
   the same class, one failure reported at more than one site.)* Export
   resolution is a graph from export names to declarations. A failure
   has one origin, and only the origin reports it:
   - a missing name in a present module: the name, S016;
   - a missing module: the `from` specifier, once per statement, with
     the code an import of a missing module gets (S100);
   - an alias cycle (a strongly connected component of re-exports with
     no declaration): one S016 for the cycle, at the member that comes
     first in program file order and then in source order; the message
     names every member of the cycle, never the path that reached it.
     A re-export that leads into a cycle is downstream, not in the
     cycle. `tsc` also reports one TS2303 per cycle.
   - a declaration that the checker rejects (an out-of-surface type
     alias, an interface, a module-level binding pattern, or any other
     rejected declaration kind): the declaration's own diagnostic. Its
     name still enters the export graph, marked failed, so a
     re-export or an import of it finds the name and is downstream;
   - an out-of-surface export form (rules 7, 7a, 7b): its S100, and its
     source module is not resolved, so a missing source adds nothing.
   Every site downstream of a failure (a re-export, a local re-export,
   or an import of the failed name) binds it poisoned and reports
   nothing. *(Corrected 2026-09-29 after the third Phase Review: a
   rejected declaration left no name in the graph, so each consumer
   reported S016.)* A declaration's duplicate name is reported once, by the
   module scope (S017, §82.2); the export-name rule of rule 6 reports
   only a collision that involves a re-export.
5a. Under the discovery option (§63 rule 5), a re-export from a missing
   module is poisoned as an import from it is.
6. A module's export names are one namespace: two exports of one name
   in one module (a declaration and a re-export, or two re-exports) are
   S017 at the second one, although a re-export binds no local name.
7. `export * from`, `export * as ns from`, and `export default` stay
   outside the surface (S100): the export surface stays explicit.
   `tsc` accepts `export * from`, so the rejection is a divergence: a
   new collision entry (C18, "the module surface is named exports and
   named imports") records it with its own divergence block.
7a. The export name `default` is outside the surface too, in every
   spelling (`export { a as default }`, `export { default } from`):
   S100 with the C18 block.
7b. Type-only export forms (`export type { T }`, `export type { T }
   from`, `export { type T }`) are outside the surface: S100 with the
   C18 block. A named export of a class, an enum, or a string alias
   exports its type as well.
7c. A mirror (`.d.ts`) file keeps its surface: an export list in a
   mirror is S100 as before.
8. The program loader follows the source module of every re-export with
   `from`, as it follows an import, so a program that reaches a module
   only through a re-export chain loads it. The file order does not
   change otherwise: module globals initialize in file order, and the
   initializer-order check reads it, so the order is program meaning.
   A re-export source takes its place in discovery order as an import
   does. One function computes that order; the CLI loader and the
   corpus reader call it. *(Corrected 2026-09-29 after the Phase
   Review: a path-order sort changed the output of a program whose
   module initializers print, and rejected a valid program.)*
9. An import can name a re-exported name: `import { b } from "./r"`
   where `./r` re-exports `b` binds the declaration of rule 1.
10. §125 rule 4 (a host entry name is unique in the program) reads the
    host entry predicate, which this section does not change: a
    re-export does not make a function a host entry, and does not stop
    one from being one. §129 changes the predicate.
11. A re-exported module global is read-only in every importing
    module, as an import is (§127).

### 128.2 Acceptance

1. Red first. Each entry is accepted or rejected as stated here after
   the change, and fails at the contract pin (S100, or a load failure);
   the round records the Red output.
2. Accept entries (directory entries, `js-comparable`, all three engines
   and `node`, goldens): a named re-export with `from` of each rule 3
   kind; a local re-export (`export { a as b }`) of a declaration and
   of a named import; a chain of two re-exports reached only through
   the chain (rule 8); a module cycle whose exports reach declarations;
   a write to a re-exported module global seen through the re-export.
3. Reject entries, each header stating the measured `tsc` result: a
   re-export of a missing name (rule 5, TS2305); an alias cycle (rule
   4, TS2303); two exports of one name in one module (rule 6, TS2300);
   `export *` (rule 7, `tsc` accepts; the header cites C18).
4. A test pins that reordering the program's input files does not
   change the resolved exports (rule 4). A test pins that the CLI
   loader and the corpus reader give one file order, and that two
   import orders of one program keep their module initialization
   orders (rule 8), with a program whose initializers print.
4a. Tests pin rules 5 (one diagnostic per failure along a chain), 5a,
   7a, 7b, and 7c, each with a firing control.
4b. Rule 5b has a total check: for every reject entry whose expected
   error is an export or import resolution diagnostic, the checker's
   diagnostic count equals the `tsc` error count measured for it (one
   S016 or S100 per TS2305, TS2303, or TS2307), recorded in the entry
   header. The check selects the entries from a structured fact that
   the resolver sets on its diagnostics, not from message text, and a
   firing control shows that a resolution entry without a recorded
   count fails the check. A test builds each rule 5b shape (a prefix
   into a cycle, a two-name re-export from a missing module, a
   declaration duplicate, a rejected declaration re-exported and
   imported, an out-of-surface form from a missing module) and
   asserts the exact diagnostic list.
5. No existing `.expected` golden moves.
