<!-- §267 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 267. Two modules cannot share a stem

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R09.
Selected by the owner on 2026-11-13.

Problem, read at `f4fa966d`. A module is named by its stem: the base name of
its file without `.ts` (`compiler/src/parse.rs` `stem_of`). A host that
calls `check_program` can pass two files with one stem, such as
`one/foo.ts` and `two/foo.ts`. The passes then disagree on which file
the stem names:

- Import resolution takes the first file with the stem
  (`compiler/src/check/signatures.rs`).
- Re-export resolution takes the first file (`compiler/src/check/exports.rs`).
- The initializer order builds a map from stem to file, which keeps the
  last file (`compiler/src/check/init_order.rs`).

So the edge that orders the initializers can name another module than
the edge that resolves the symbols. The order of initializer side
effects can change, or a false initialization-order error can occur.
`SourceFile` documents the stem resolution and states no rule that
stems are unique.

### 267.1 Rules

1. `check_program` rejects two source files that are not ambient and
   have the same stem, with one diagnostic at the second file that
   names the stem and the two file names. No pass runs on such a set.
2. Ambient files (`.d.ts`) keep their present rules.
3. The documentation of `SourceFile::name` states rule 1.
4. The CLI loader cannot load two files with one stem, as it resolves an
   import in the directory of the entry. Rule 1 does not change a
   program that the CLI loads.
5. §154 rule 2 is revised: a rejection site has one of three classes.
   The third class is `ProgramInput`: no source program reaches the
   site, because it rejects the set of files or the options that the
   host passes, and `tsc` has no such input. The diagnostic carries no
   variant. The duplicate stem of rule 1, an empty set of files, and an
   unknown enabled module are `ProgramInput` sites.

### 267.2 Acceptance

1. Red first, at the pin: a unit test passes `one/foo.ts` and
   `two/foo.ts`, each with a module initializer that prints its
   directory, and an entry that imports `./foo`. At the pin the check
   succeeds. The tracking note records the initializer order and the
   module that the import resolves to.
2. After the change, the check fails with the diagnostic of rule 1.
3. A control: two files with different stems in two directories
   check, and the initializer order holds each module.
4. No corpus entry or golden changes. The diagnostic code is an
   existing code if one fits; the unit test pins it.

### 267.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each entry point that takes a source
set; each reaches the check of rule 1 before a pass runs. The check
reads the stem with `stem_of`, so the two cannot differ.

1. MINOR, contrived, fixed: the implementation added the class
   `ProgramInput`, and no contract named it. Rule 5 states it.
2. MINOR, contrived, recorded: §267.4 item 1.
3. MINOR, fixed: acceptance item 3 asked for a run of the control, and
   the test checks it and reads its initializer order.

### 267.4 Open

1. An ambient file can have the stem of a module, as before this
   section. The import lookup takes the first file with the stem and
   does not skip an ambient file.
