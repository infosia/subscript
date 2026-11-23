<!-- §354 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 354. A module is named by its path

*(Added 2026-11-23.)* Origin: a downstream change request of
2026-11-23, with probes run at `75f49b2a`.

Problem, measured at `75f49b2a` with the CLI:

1. **A program cannot import a module from another directory.**
   `sub/x.ts` exports a function. Each of these imports is S100
   "imported module is not among the program's files", and `tsc`
   accepts each form: `./sub/x` from `main.ts`; `./sub/a` from
   `main2.ts`, where `sub/a.ts` imports `./x`; `../sub/x` from
   `other/up.ts`.
2. **A module is named by its stem**, the base name of its file
   without `.ts` (`compiler/src/parse.rs` `stem_of`). The checker
   removes `./` and `.ts` from a specifier and compares the result
   with each stem. A specifier with `/` matches no stem.
3. **The directory of the importer is not read.** Through
   `check_program`, an entry `sub/a.ts` that imports `./x` checks
   when the only other file is `unrelated/x.ts`.
4. **Two files with one base name cannot be in one program** (§267
   rule 1), such as `world/chunk.ts` and `render/chunk.ts`.
5. **The resolution rule has more than one copy.** The CLI loader
   follows only `./name` in the directory of the importer
   (`cli/src/program_loader.rs`). The corpus loaders have a rule of
   their own. A host that loads modules from disk must copy the rule,
   and no check compares the copies.

### 354.1 Rules

1. **The key of a module is its path.** The key of a source file
   that is not ambient is its `name`, normalized, without the suffix
   `.ts`. The separator is `/`. The normal form has no empty segment
   and no `.` segment, and a `..` segment removes the segment before
   it. A `..` segment with no segment before it stays. `a/./x.ts`,
   `a//x.ts`, and `a/b/../x.ts` have the key `a/x`.
2. **A relative specifier resolves against the importer.** A
   specifier that starts with `./` or `../` is joined to the directory
   of the key of the file that holds it, and the result is normalized
   by rule 1. The import, the re-export, and the namespace import
   name the file that has this key. If no file has the key, the
   present diagnostics apply (S100 at the specifier).
3. **A specifier that is not relative names no file of the program.**
   The standard modules keep their rules (§185, §193). At the pin a
   bare specifier `x` resolves to the stem `x`, and `tsc` rejects it
   (TS2307, measured). An import lookup does not read an ambient
   file.
4. **A `.ts` suffix of a specifier keeps its present treatment.**
   `./x.ts` names the key `x`. `tsc` rejects the form (TS5097,
   measured). This section changes nothing for it.
5. **Two files with one key are rejected.** The check of §267 rule 1
   compares keys. `a/util.ts` and `b/util.ts` are two modules.
   `a/x.ts` and `./a/x.ts` are one key, and the set is rejected. The
   diagnostic names the key and the two file names.
6. **One resolver, and it is public.** The compiler crate exports the
   function that gives the key of a name and the function that
   resolves a specifier against the name of an importer:

   ```rust
   pub fn module_key(name: &str) -> String;
   pub fn resolve_module_specifier(importer: &str, specifier: &str) -> Option<String>;
   ```

   The second function returns `None` for a specifier that is not
   relative. Each pass of the checker that reads a specifier calls
   it: the import resolution, the re-export resolution, the
   initializer order, and the lookup of a poisoned module. The CLI
   loader and the corpus loaders call it. No other code removes `./`
   or `.ts` from a specifier.
7. **An entry of `CheckOptions::poison_missing_modules` is a key.**
   The caller writes it as a specifier relative to the root of the
   names (`./p.typegpu`), and the compiler normalizes it by rule 1.
   An import whose resolved key is in the list is poisoned. A
   specifier that is not relative has the poison key that rule 1
   gives its text, so a bare specifier can be poisoned as at the pin.
   The name of an ambient `declare module "./other"` resolves against
   the ambient file that holds it.
8. **The CLI loads each relative import.** The loader follows `./a/b`
   and `../a`, transitively, and finds each file with the resolver of
   rule 6. It names each file by the path that the imports spell,
   from a root, with `/` separators, on each host: the name is not
   the canonical path of the file. So the key of each name is the key
   that rule 2 gives from the name of its importer. The root is the directory of the entry,
   moved up by the largest count of leading `..` segments in the key
   of a specifier that loaded a file, resolved from the directory of
   the entry. So an entry `sub/m.ts` that imports `../sub/x` has the
   names `sub/m.ts` and `sub/x.ts`. A program with no `..` that
   leaves the directory of the entry has the entry at the top, as at
   the pin. A program with no import keeps its present name. If the
   root cannot move up that far, the CLI reports a usage error. A
   specifier that names a directory, or whose key is empty or ends in
   `..`, loads no file.
9. **A derived name is unique for each key.** Two modules with one
   base name can each declare a function, a class, an enum, and a
   module variable with one name. The identity symbols, the emitted C
   names, and the names of the dev JIT stay different. A diagnostic
   shows the `name` of the file.
10. **A corpus entry can hold subdirectories.** A directory entry has
    `main.ts` at its top. Each loader of the corpus reads the `.ts`
    files below the directory, names each by its path from the entry
    directory, and resolves with rule 6: the differential harness,
    the capture tool, the `tsc` gate, the `node` gate, and each other
    loader that reads a directory entry. A file that no import
    reaches still fails the entry.
11. **A hot reload watches each loaded file.** A change of a file in
    a subdirectory starts a reload, as a change of a file beside the
    entry does.

### 354.2 What does not change

- A program whose files are in one directory, with `./name`
  specifiers, has the keys, the names, the initializer order
  (§137), and the output that it has at the pin.
- An ambient file has no key (§267 rule 2).
- The entry module is the host API (§129). The directory of an
  imported module does not change what the host sees.
- The language has no path alias, no directory import (`./sub` for
  `./sub/index.ts`), and no package lookup.

### 354.3 Acceptance

Red first, at the pin, for each item that states it.

1. A unit test of the two functions of rule 6: `./x`, `../x`,
   `./a/../b`, `./a/./b`, a specifier that leaves the root (`../x`
   from `main.ts` gives `../x`), a bare specifier, a standard module,
   an importer in a subdirectory, and an importer whose name has a
   `.` segment.
2. An accept entry `a510` with `main.ts`, `a/util.ts`, `b/util.ts`,
   and `a/inner.ts`. `main.ts` imports each `util` module.
   `a/inner.ts` imports `./util` and `../b/util`. The two `util`
   modules each declare a function, a class, an enum, and a module
   variable with one name, and each has an initializer that prints.
   The entry has a re-export through a path and a namespace import of
   a module in a subdirectory. The output shows a value from each
   declaration and the initializer order. Red at the pin (S100). The
   output is equal in the three tiers. The entry type-checks under
   `tsc` and has the `js-comparable` header.
3. A reject entry `r563`: `main.ts` imports `./sub/a`, `sub/a.ts`
   imports `./x`, and the only `x.ts` is beside `main.ts`. S100 at
   the import in `sub/a.ts`. The header states what `tsc` does,
   measured. Red at the pin is the form of problem 3, in a unit test:
   the set checks at the pin.
4. A unit test: `./sub/missing` is S100 at the specifier.
5. The test of §267 moves to keys: `one/foo.ts` and `two/foo.ts`
   check as two modules, and an import of `./one/foo` reaches the
   first. `a/x.ts` and `./a/x.ts` are rejected with the diagnostic of
   rule 5.
6. CLI tests on files in a temporary directory, on the dev tier and
   the ship tier: the three probes of the problem print their values;
   `other/up.ts` shows that an import can leave the directory of the
   entry; a diagnostic in `sub/x.ts` shows the name `sub/x.ts`. Red
   at the pin.
7. A CLI or `ReloadSession` test of rule 11: a change of a file in a
   subdirectory gives the new output.
8. A test of rule 7 with an importer in a subdirectory.
9. `tools/gate.sh full` exits 0 and moves no golden. Each test states
   its cost.

### 354.4 Open

1. The CLI reads one file one time, and the identity of a file is its
   canonical path. A file that two spellings reach (another case on a
   file system that ignores case, or a symbolic link) has the name of
   the first spelling. The second spelling is S100. Measured: `./Util`
   and then `./util` for one file `util.ts`.
2. A host that calls `check_program` selects the names. If it names
   the entry `main.ts` and another file `/abs/sub/x.ts`, no relative
   specifier reaches the second file.
3. A specifier with a `/` at its end, `./sub/`, has the key `sub` and
   names `sub.ts`. `tsc` reads it as a directory *(docs)*.
4. The CLI names a mirror by the path that the command line gives. A
   relative `declare module` in a mirror has a key from that path,
   and no import of the program resolves to it. No mirror in the
   repository has that form.
5. The `#[cfg(windows)]` test of the names is not compiled on the
   host of this section.
