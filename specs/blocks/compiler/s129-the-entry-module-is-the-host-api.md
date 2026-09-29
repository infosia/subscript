<!-- §129 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 129. The entry module is the host API

*(Added 2026-09-29.)* Origin: §125 left one program-wide name, the host
entry. The owner chose the entry module as the host API on 2026-09-29,
with named re-exports (§128) and function-only exports from the entry
module.

Problem: an `export` does two jobs. It makes a name importable by
another module, and it makes a function callable by the host. Every
exported function in any module whose signature qualifies is a host
entry (`hir::Function::host_entry_trap_sites` is `Some`) and gets
`subscript_export_<name>` with no module qualifier. So two modules
cannot both export an `update` helper (§125 rule 4, S017), and the
host API has no single place that lists it.

### 129.1 Rules

1. The entry module is the program file that the build names: the
   `source` argument of `subscript run`, `check`, and `build`. The
   library API names it explicitly (a field of the program input); it
   is not inferred from file order. A program with one non-ambient
   file has that file as its entry module; a program with more than
   one names it, or the check fails with a diagnostic that says so.
   An ambient `.d.ts` file cannot be the entry module.
2. A host entry is a function that the entry module exports, declared
   and exported there or re-exported there (§128). The host entry name
   is the export name, not the declaration name.
3. The entry module's export names are one namespace (§128 rule 6), so
   two host entries of one name are S017 there. §125 rule 4 (a
   program-wide host-entry rule) is removed.
4. An exported function of the entry module that cannot be a host
   entry is an error at the export, with the reason: a §59 signature
   violation, a generic function, a generator, or an async function
   with parameters. A re-export chain that exposes such a function is
   an error at the exposing export, and the message names the target
   declaration.
5. A zero-argument async function keeps its host entry form (§59).
6. The entry module exports functions only. An exported class, enum,
   module global, string alias, or generic declaration of the entry
   module, declared or re-exported, is an error at the export. The
   entry module can declare these without `export` and use them.
7. An export of any other module is a module export only. It emits no
   `subscript_export_*` adapter, no dev-session entry, and no generated
   header declaration.
8. Two export names of the entry module that resolve to one function
   are two host entries. Each gets its adapter; they share one
   implementation and the module state.
9. The symbol stays `subscript_export_<name>`, with no module
   qualifier. §125.3 (a module-qualified symbol) is withdrawn.
10. `main` is not required. When the CLI or a host calls `main`, the
    entry module exports it and it satisfies the entry rules; a missing
    `main` in that case is a clear error. *(Made precise 2026-09-29
    after the second Phase Review: three CLI paths gave three different
    errors.)* The checker answers one query, "the runner `main`": the
    host entry named `main`, or a diagnostic (S100, at the entry
    module) that says the entry module exports no host entry `main`.
    `subscript run`, `subscript run --watch`, and `subscript build`
    (with or without `--run`) read that one answer, so each path gives
    the same diagnostic.
10a. The async roots are the targets of the async host entries,
    one per target identity: two export names of one async function
    kick it once. The target of the runner `main` is not a root, since
    the runner calls it. The roots run in the order of the entry
    module's export sites in source order (the first export site of
    each target), so the order does not depend on the order of the
    program's input files. In a one-module program this is the
    declaration order that §26 states. Every tier reads the one root
    list, and so does every runner path: `subscript run --watch` kicks
    the roots and pumps them as `subscript run` does, so the two print
    the same output. A missing runner `main` under `run --watch`
    renders the rule 10 diagnostic and the watch continues, as
    `cli.md` §12.1 states for any diagnostic of the initial program.
11. One host entry table carries each host entry: the public name, the
    target declaration identity, the checked boundary signature, and
    the export position. Dev lookup, ship adapters, the generated
    header, the entry roots, and hot reload read that table; the
    predicate `host_entry_trap_sites(..).is_some()` no longer decides
    membership. A reload compares the public names and boundary
    signatures, and resolves the target implementations.
12. Every host entry is kept even if no script function calls it.
13. The program header, emitted with the program, is the one owner of
    the host entry declarations. The static runtime header
    (`runtime/include/subscript_runtime.h`) declares the runtime API
    only and no `subscript_export_*` symbol, so a host that includes
    both headers sees one declaration of each entry. A host that calls
    an entry includes the program header. *(Added 2026-09-29 after the
    Phase Review: the static header declared `subscript_export_main(ctx)`,
    which conflicted with a `main` that takes parameters.)*
13a. *(Added 2026-09-29 after the third Phase Review: hand declarations
    remained in test and runner support.)* No C source in the
    repository declares a `subscript_export_*` symbol by hand: not a
    host, not a test host, not the AOT runner entry, not a benchmark.
    The emitted `program.c` includes its own program header, so a
    declaration that differs from its adapter definition fails to
    compile. The test-host helper takes the program header, so a test
    host cannot be built without it. A total check scans every C source
    and C string in the repository for a hand declaration of an entry
    and fails on one, with a firing control.

### 129.2 Acceptance

1. Measure first: every corpus entry and example is checked and run
   with the new rule. The round reports every entry whose acceptance or
   host symbol set changes; no `.expected` golden moves. Measured on a
   prototype at `9815325`: two accept entries became rejected under
   rule 4 (`a143`: the entry exports the generic async `tick<T>`;
   `a281`: the entry exports `read(): i32`), and `bump` left the host
   name sets of `a287` and `a288` (a non-entry export). The owner chose
   on 2026-09-29 to keep rule 4 and rewrite both entries: `a143` moves
   `tick` to a sibling module (a directory entry) and keeps its output;
   `a281` drops the `export` on `read`.
1a. `r267-duplicate-host-entry` (§125 rule 4) retires: its program, two
   modules that each export `update`, is accepted under rule 3 (only
   the entry module's `update` is a host entry). The same program
   becomes an accept entry, and the retirement is recorded as the
   corpus records other retirements.
2. Accept entries (directory entries, goldens, all engines and `node`
   where comparable): two library modules export `update`; the entry
   module re-exports them as `physicsUpdate` and `audioUpdate`; no bare
   `update` host entry exists. One function exported under two names:
   both host calls reach one implementation and one state.
3. Reject entries, each with its measured `tsc` result and collision
   citation (C18 where `tsc` accepts): a non-function export of the entry module (rule 6); an
   entry export with an unsupported signature, direct and through a
   re-export chain (rule 4); two entry exports of one name (rule 3).
4. Tests: a host entry with no script caller stays callable; a host
   API without `main` works, and a missing `main` is the one rule 10
   diagnostic on every CLI path (run, run --watch, build, build
   --run); the async roots of rule 10a do not change when the input
   files are reordered, across modules; a zero-argument async entry keeps its behaviour; the
   generated header and the dev lookup list the same names; reordering
   input files does not change the entry module or the host entries.
5. Reload tests: a change of an alias target and a change of a public
   signature, under the existing reload compatibility rules.
6. The implementation commit updates the records: C14 drops the
   host-entry paragraph of §125; C18 records rules 4 and 6 as
   divergences (`tsc` accepts those exports), with their reject
   entries; the S017 row of §6 drops §125 rule 4 and cites this
   section. §59 rules 1 and 4 and Q12 are corrected in this contract
   commit. §5 (the symbol prefix) does not change.
