# §130 — ValueType decorator rename

Contract: `specs/blocks/compiler/s130-the-value-class-decorator-is-valuetype.md`.

## Red at `6ce5dfe`

The CLI binary built from the contract pin accepted `r280` before the checker change.
The run used a binary built from `3814789`. `6ce5dfe` differs from it only in spec files, so the binary is the same.

```text
check: corpus/reject/r280-cstruct-renamed.ts: no errors
```

Exit code: 0.

## TypeScript measurement

TypeScript with the renamed prelude rejected `r280` with exit code 2:

```text
corpus/reject/r280-cstruct-renamed.ts(7,2): error TS2304: Cannot find name 'CStruct'.
```

## Golden exception

`a255-json-parse-direct.expected` line 8 changed from `CStruct: 3 true` to `ValueType: 3 true`.
All other golden contents and the LIR text snapshot must stay byte-identical.

## Verification

The reject test pins S100 at the decorator and the message naming `@ValueType`.
The source scan includes a firing control and restricts the former spelling to the rejection witness.
The whole tracked-tree scan found only the S100 diagnostic and its test outside the permitted spec records.
`cargo fmt --check`, the warning-free workspace build, the release benchmark build, `tsc -p tsconfig.json`, and `tools/hygiene.sh` passed.
Golden contents differ only at the permitted `a255` line. The LIR text snapshot is byte-identical.

## Full gate 1 at `813a2e6`

```text
gate full 813a2e6 dirty:116 debug 1959/2/3 release 1957/1/3 skips 2/0 clippy 3/18/13 goldens-moved 4 exit 1
```

Two failures.

1. `tsc_corpus.rs` `every_corpus_tsc_header_matches_measured_tsc` (both profiles). The `r280` header
   `// tsc: rejects TS2304 (measured with the renamed prelude)` is not the form `rejects TS<code>[, TS<code> ...]`.
   The header is now `// tsc: rejects TS2304`. The `expected-error` header is now
   `S100 at line 7, the decorator names @ValueType`, the `S<code> at line <n>` form of the other reject entries.
2. `cli/tests/watch.rs` `spawned_watch_preserves_stdout_before_each_trap` (debug). The test waited 1,579 s at 0% CPU.
   It failed only when the child was stopped by hand.

## Cause of the watch failure

`run_watch` took the initial file stamps after `write_watch_step`, so after the program output.
The test writes its edit when it reads that output. If the edit lands before the stamp, the stamp records the edited file.
No later poll then sees a change, and the test waits forever. The edit keeps the file length (`3` to `4`), but a length change is also recorded by a late stamp.
In the poll loop the stamps of a reload were also taken after `load_program`, so a write between the read and the stamp was lost.

Evidence. The test alone passed 5 of 5 runs at about 1.0 s.
A prototype added a 500 ms sleep between `write_watch_step` and the initial stamp. It failed 2 of 2 runs with the gate's message:

```text
input ended before "watch: swapped\n" reached 1 time(s); saw 0; captured:
watch: started
subscript: main.ts:13:28: trap [index-out-of-bounds]: index 1 out of bounds for array length 1
```

Both processes sat at 0% CPU for 10 minutes. The watched `main.ts` contained the edit `while (i < 4)`. Each run ended only when the child was stopped.

## Fix

`WatchedFiles::after_load` gives each loaded path the stamp taken before the load that read it.
A path that no stamp precedes gets `FileStamp::Unseen`, so the next poll reloads it once. `WatchSession::step` returns `Unchanged` for identical sources.
The initial load stamps the entry before `load_program`. The poll loop keeps `refresh` before the load and calls `after_load` after it.
The unit test `watched_files_keep_the_stamp_taken_before_the_load` writes a file between the stamp and `after_load` and expects a change.

With the fix and the same 500 ms prototype sleep, the test passed 3 of 3 runs at about 1.5 s. The prototype sleep is reverted.
The `watch` suite passed 3 of 3 runs.

## Bound on a missing line

No per-test bound is added. §102 rule 2 and rule 3b forbid a per-test deadline; `tools/gate.sh` bounds each step at 3600 s.
The gate record shows the failure at 1,579 s, before that bound, because the child was stopped by hand.

## Phase Review fixes

1. MAJOR, core principle 9. `watched_files_keep_the_stamp_taken_before_the_load` passed when `after_load`
   stamped known paths again, because the second path was `Unseen` and `changed()` was true for that reason.
   The test now uses only the known path: stamp, write, `after_load(vec![known])`, then `changed()`.
   `watched_files_mark_a_path_without_a_stamp_unseen` keeps the `Unseen` case.
   The initial load moves into `initial_watch_load`, which takes a loader. `initial_watch_load_stamps_before_the_load`
   writes the entry inside the loader and expects a change, for a loaded program and for a program error.
   Firing controls, each applied and reverted:

   | Mutation | Failing tests |
   |---|---|
   | `after_load` uses `.map_or(FileStamp::Unseen, \|_\| file_stamp(path))` | `watched_files_keep_the_stamp_taken_before_the_load`, `initial_watch_load_stamps_before_the_load` |
   | loaded branch uses `WatchedFiles::new(paths)` | `initial_watch_load_stamps_before_the_load` |
   | program-error branch uses `WatchedFiles::new(vec![entry])` | `initial_watch_load_stamps_before_the_load` |

2. MINOR. `compiler/tests/decorator_spelling.rs` scans every file that `git ls-files --cached --others --exclude-standard` lists.
   It excludes only `specs/tracking/`, `specs/blocks/compiler-history.md`, the §130 contract, and named lines.
   The match ignores ASCII case and also applies to paths. The entry name `r280-cstruct-renamed` is permitted everywhere.
   The scan reads bytes, so a non-UTF-8 file is scanned and named on stderr. The scan found none.
   The test asserts that the scan read files and read `prelude/lang.d.ts`.
   The enumeration firing control scans the same files without exceptions and expects a hit in each excepted file and directory.
   Cost: one `git ls-files` call and one read of 1,792 files, about 0.5 s in debug.
   Firing controls, each applied and reverted:

   | Mutation | Result |
   |---|---|
   | `see cstruct` appended to `README.md` | fails, `README.md:440` |
   | enumeration limited to `compiler/` | fails, `prelude/lang.d.ts` not read |
   | enumeration without `generated-docs/` | fails, no unexcepted hit in `generated-docs/corpus-index.md` |

3. MINOR. `@CStruct({ align: 8 })` gave the general decorator message. `is_former_value_decorator` matches the bare
   and the call form, and both give S100 `` `@CStruct` was renamed to `@ValueType` ``.
   `former_value_decorator_call_names_its_replacement` pins it. Without the call form the test fails with
   `` the only decided decorators are the ambient `@ValueType` and `@Descriptor` ``.

4. MINOR. The Red section cites `6ce5dfe`.
