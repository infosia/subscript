<!-- §279 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 279. The frame-root corpus check runs its entries on a pool

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P1, "parallelize the frame-root corpus analysis").
Iteration speed is a goal, and a gate's cost is part of its design
(core principle 15).

Problem, measured. The library test
`every_corpus_coroutine_field_matches_live_values_at_suspension`
(`codegen/src/lower/func/frame_root_tests.rs`) checks each accept entry
and each trap entry, lowers it, and for each entry with an async or a
generator function emits C and Cranelift code and compares the frame
fields. It runs the entries one after another on one thread. Alone, it
took 35.59 s in debug on 2026-11-14. The whole `subscript-codegen`
library test binary took 45.56 s in debug in the full gate record of
2026-11-13.

### 279.1 Rules

1. The test runs its entries on a bounded pool of worker threads, at
   most `std::thread::available_parallelism()` workers. Each work item
   is one corpus entry.
2. A work item owns its compiler state, its JIT module, and its
   captured Cranelift text. It takes the captured text on the worker
   thread that lowered the entry (the capture is thread-local in
   `codegen/src/lower/func.rs`).
3. A work item returns its findings and its counts. The test thread
   reports the findings in corpus order, and each finding names its
   entry. The test fails if any finding exists.
4. Each comparison of the test stays as it is. The entry set stays:
   each accept entry and each trap entry.
5. The pool uses no process-wide state that another item changes. The
   test states each shared state that a work item reads, and why it is
   safe to share.
6. The function count and the suspension count that the test asserts
   or prints stay the same before and after.

### 279.2 Acceptance

1. A control: a finding in one entry fails the test, and the failure
   names the entry. The control injects the finding through the
   finding path of rule 3, not through the comparisons.
2. `cargo test -p subscript-codegen --lib
   every_corpus_coroutine_field_matches_live_values_at_suspension`
   passes in debug and in release. Its wall time, with the binary
   already built, is recorded before and after the change in
   `specs/tracking/test-cost-review-2026-11-14.md`, with the counts of
   rule 6.
3. The full gate passes. No golden moves.

### 279.3 Phase Review

Pending.

### 279.4 Open

None.
