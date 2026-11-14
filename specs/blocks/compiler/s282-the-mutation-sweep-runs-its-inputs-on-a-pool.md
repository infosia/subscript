<!-- §282 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 282. The mutation sweep runs its inputs on a pool

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P2, mutation robustness, its first and third steps,
which keep every input). Iteration speed is a goal, and a gate's cost
is part of its design (core principle 15).

Problem, measured on 2026-11-14 in debug, with timing added to a copy
of `compiler/tests/robustness.rs` and then removed. The test
`corpus_mutations_do_not_panic` took 29.5 s:

| Part | Seconds |
|---|---:|
| Checker calls, one thread | 28.86 |
| Companion lookup | 0.27 |
| Mutation generation | 0.08 |
| Source discovery and reading | 0.03 |

The test made 38,563 checker inputs from 1,095 sources. 38,546 inputs
are distinct within their source. The companion lookup scans the whole
source list once for each source (1,095 scans). The checker work is
necessary, and it runs on one thread.

### 282.1 Rules

1. The test runs its checker inputs on a bounded pool of worker
   threads, at most `std::thread::available_parallelism()` workers.
2. The set of inputs does not change: each source, each mutation kind,
   each position, and each companion file set stay as they are.
3. Each panic is recorded with the label of its own input (the
   corpus-relative path, the mutation kind, and the position). The
   label is per thread, so a panic on one worker cannot take the label
   of another worker's input.
4. The test indexes the companion files by directory once, before the
   pool starts.
5. The test still restores the previous panic hook before any
   assertion, and it still fails on a recorded panic.

### 282.2 Acceptance

1. A control: with a temporary checker stub that panics for one chosen
   input among several inputs on several workers, the recorded panic
   holds the label of that input, and no other label. The control
   lands as a test of the harness, not of the checker.
2. `cargo test -p subscript-compiler --test robustness` passes in
   debug and in release. It prints the same source count and input
   count as before. Its wall time, with the binary already built, is
   recorded before and after the change in
   `specs/tracking/test-cost-review-2026-11-14.md`.
3. The full gate passes. No golden moves.

### 282.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. It found that the
input set and the companion order are the same, that each panic takes
the label of its own input, and that one lock keeps the control and the
sweep from sharing the process panic hook. Three MINOR findings, each
fixed:

1. Contrived: the control passed by chance against a shared label
   slot, and tested nothing with one worker. It now runs at least two
   workers, and the chosen input waits until each other worker has set
   its label.
2. Realistic: a failing control poisoned the hook lock, and the sweep
   then failed with a second message. The lock now recovers the value.
3. Realistic: each input of the corpus was built before the pool
   started. A work item is now one source, so a worker holds the
   inputs of one source.

### 282.4 Open

None.
