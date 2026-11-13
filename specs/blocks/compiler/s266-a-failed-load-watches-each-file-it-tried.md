<!-- §266 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 266. A failed load watches each file it tried

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R07.
Selected by the owner on 2026-11-13.

Problem, read at `cc255537`. `subscript run --watch` (`cli/src/lib.rs`
`run_watch`) watches the files of the last load that succeeded. If the
first load fails with a program error, it watches the entry only
(`initial_watch_load`). If a later load fails, it keeps the old watch
set. Neither path watches a file that the failed load found and could
not load or parse. If an edit of the entry adds an import of a new
module that does not parse, and the developer then fixes only that
module, no reload runs. The developer must touch a file of the old set.
The review measured it: the fix of the module alone gave no reload,
and a touch of the entry ran the fixed program.

### 266.1 Rules

1. A load reports the path of each source file that it tried to read:
   each file that it read, the file that failed to parse, and a path of
   an import that does not exist.
2. After a load that fails, the watch set is the union of the paths of
   rule 1 for that load and the entry.
3. A path that does not exist is watched for its creation. When the file
   appears, the next poll loads again.
4. After a load that succeeds and a check that accepts the program, the
   watch set is the files of that load, as now. After a load that
   succeeds and a check that rejects the program (for example an import
   of a module that does not exist), the watch set is the union of rule
   2.
5. The loader runs no check. It returns the paths of rule 1 with a
   successful load too. The output, the diagnostics, and the exit code
   of `run`, `build`, `emit`, `check`, and `boundary` do not change.
6. A usage or read failure of the entry at startup exits 2, as
   `specs/blocks/cli.md` §12.1 states.
7. The stamp of each path of rule 1 is the stamp read before the load.
   So a failed load does not make the next poll load again when no file
   changed, and a failure is printed once for each change.

### 266.2 Acceptance

1. Red first, at the pin: a unit test of the watch loop (with the load
   and the clock under the test's control, as the present watch tests
   do) builds an entry that imports a module that does not parse. The
   test fixes the module only, and no load runs. The tracking note
   records it.
2. After the change, the fix of the module alone runs a load, and the
   program runs.
3. The same test with an import of a module that does not exist: the
   creation of the file runs a load.
4. A control: an edit of a file that no load tried runs no load.
5. The test runs no real sleep longer than the present watch tests.
   It states its cost.
6. No corpus entry or golden changes.

### 266.3 Phase Review

One fresh review of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed: the loader of the first round ran a check
   with the default options when an import was missing, to return the
   paths it tried. A program that enables `node:fs/promises` then got a
   false diagnostic in `run`, `build`, and `check`. Rule 5 states that
   the loader runs no check, and rule 4 gives the watch set after a
   rejected check.
2. MAJOR, realistic, fixed: a missing entry at startup of `run --watch`
   waited with no end, and `specs/blocks/cli.md` §12.1 gives exit 2.
   Rule 6 states it.
3. MINOR, realistic, fixed: a failed load gave each new path a stamp
   that the next poll saw as a change, so the failure was printed
   twice. Rule 7 states the stamp.

### 266.4 Open

None.
