<!-- §268 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 268. A retained-output write error fails the run

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R11.
Selected by the owner on 2026-11-13.

Problem, read at `6b786ccb`. On Unix the development tier runs the program
in a forked child. The child writes each printed line through to a file
that the parent owns, so the output survives a hard stop of the child
(`codegen/src/jit/output.rs` `capture_stdout_line`). The write-through
discards each error of `write_all` and `flush`. The child then writes a
successful outcome to the outcome file (`codegen/src/jit/entry.rs`),
and the parent returns the text of the retained file. If the file system
of the retained file is full or fails, the run succeeds with output
that is short. A consumer loses a line, or reports a false difference of
output.

### 268.1 Rules

1. The write-through keeps the first error of a write or a flush, and
   stops the write-through after it. The in-memory copy of the output
   continues.
2. At the end of the entry, a kept error makes the run fail with an
   internal error that names the retained file and the error. The
   outcome does not report success.
3. If the run ends with a trap too, the report names both. If the child
   stops abnormally after a write error, it cannot report, and the
   report of the abnormal stop gives the output prefix that the file
   holds, as before this section.
4. With no write error, the output and the outcome do not change.

### 268.2 Acceptance

1. Red first, at the pin: a unit test gives the write-through a writer
   that fails after N bytes (a test double of the file, or a file on a
   path that refuses writes), runs a program that prints more than N
   bytes, and gets a successful run with short output.
2. After the change, the run fails with the error of rule 2.
3. A control: the same program with a writer that does not fail gives
   the full output and success.
4. The test does not need a full file system. It states its cost.
5. No corpus entry or golden changes.

### 268.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read the outcome protocol: the child writes
the internal error, and the parent returns it, not the text of the
file. A child that stops abnormally still returns the prefix of the
file.

1. MINOR, realistic, fixed: the report of the error had a panic path
   in library code.
2. MINOR, realistic, fixed: a test printed a line left from the Red
   record.
3. MINOR, contrived, fixed: rule 3 did not state the case of a child
   that stops abnormally after a write error.
4. MINOR, contrived, recorded: §268.4 item 1.
5. MINOR, realistic, recorded: §268.4 item 2.

### 268.4 Open

1. The tests make the retained file through `RetainedOutput::new`, which
   reads `SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE`. If the test environment
   sets it, the tests use that file.
2. No test sends the error through the fork and the outcome protocol.
   The protocol path is older than this section.
