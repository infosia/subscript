<!-- §347 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 347. A received array has a test of the mark, and `arrops.rs` is under the file limit

*(Added 2026-11-22.)* Origin: §344.6 items 1 and 2.

Problem, measured at `5bbb167c`:

1. **No test counts the words that a collection reads for a received
   array.** A received array takes its fact from one flag of its
   schema (§344 rule 5, `runtime/src/ffi/receive.rs`). §344 acceptance
   2 counts the words for arrays that the runtime creates
   (`runtime/src/context/array_fact_tests.rs`). A receive that loses
   the flag makes each collection read each word of the buffer:
   524,288 words and about 7 ms for 4 MB (§344.5). No gate reads that
   cost: no workload of `perf-gate` receives an array.
2. **`runtime/src/arrops.rs` has 2,575 lines.** The limit is 2,000
   (§5.y rule 2). The test module of the file is 871 of them. No other
   Rust source file is over the limit.

### 347.1 Rules

1. **A runtime test receives a scalar array and counts the words.**
   The test calls `subscript_rt_boundary_receive` with a schema that
   states "no handle", for an `i32` buffer of 4 MB and one of 8 MB.
   The words that one collection reads do not grow with the length.
   Each element of the received array reads the value of the C
   buffer after the collection.
2. **The same test has a firing control.** A receive of the same
   buffers with a schema that does not state "no handle" reads a
   count that grows with the length.
3. **The test is Red under one mutation.** With the read of the flag
   removed from the receive, the test of rule 1 fails. The round
   records the failure text and restores the line.
4. **No production code changes for rules 1 to 3.** If the test of
   rule 1 fails at the pin, the round stops with a report.
5. **`arrops.rs` is split by the method of §5.y rule 2.** The test
   module moves to `runtime/src/arrops/tests.rs`. The callback
   operations move to one child module, and `arrops` exports the same
   names. The split changes no signature, no behaviour, and no path
   that a caller writes. Each file is under 2,000 lines.
6. **The split is a commit of its own** (§5.y rule 2a), before the
   commit of the test.

### 347.2 Acceptance

1. The tests of rules 1 and 2 pass in both kinds of Context (the dev
   kind and the ship kind), as the tests of §344 acceptance 2 do.
2. The mutation of rule 3 fails the test; the record states the text.
3. No Rust source file of the tree has over 2,000 lines.
4. `runtime/tests/` and each caller of `arrops` compile with no
   change of their own.
5. `tools/gate.sh full` passes. No golden moves.

### 347.3 Sections this one amends

- §344.6: items 1 and 2 close.
