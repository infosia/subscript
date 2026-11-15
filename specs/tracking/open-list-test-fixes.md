# Tracking: test and tooling fixes from open lists (§302)

Contract: `specs/blocks/compiler/s302-test-and-tooling-fixes-from-open-lists.md`.
Origin: the open lists of §175, §260, §268, §280, §283, and §296. On
2026-11-15 the owner told Claude to close the open items that a small
change closes first.

## Red and fix, by rule

| Rule | Files | Before the change |
|---|---|---|
| 1 | `codegen/src/reload/scratch_exit_tests.rs` | a missing count file read the count of the run before |
| 2 | the same file; `cli/tests/example_hosts.rs` | a failed assertion left the scratch directory |
| 3 | `codegen/src/lower/func.rs`, `codegen/src/cemit.rs`, `codegen/src/cemit/marshal.rs` | two fields with no doc comment; one emitted C check at the outer indent |
| 4 | `examples/tests/gate.rs` | the fixture `Drop` panicked when the removal failed |
| 5 | `cli/tests/example_hosts.rs` | a mismatch printed bytes and no golden path |
| 6 | `tools/downstream.sh`, `cli/tests/downstream_script.rs` | a gate that staged `Cargo.lock` left it staged |
| 7 | `compiler/tests/counted_capture_block.rs` | no rejected control; the new control gives S009 |
| 8 | `codegen/src/jit/output.rs`, `codegen/src/jit/entry.rs`, `codegen/src/jit/isolation.rs` | the tests read `SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE` |

Rule 6, measured on scratch git repositories by the Phase Review: with
a gate that stages a changed `Cargo.lock` and exits 0, 7, by SIGTERM,
or 3 after a later write, the script before the change left the index
and the working file changed. After the change, each is the committed
file, and each exit status is the same.

## Phase Review

One fresh review: no CRITICAL or MAJOR finding, six MINOR findings;
five fixed, one recorded; see §302.3. Rule 8 now passes the output path
from the caller (`RetainedOutput::open`), so a test cannot read the
environment by construction.
