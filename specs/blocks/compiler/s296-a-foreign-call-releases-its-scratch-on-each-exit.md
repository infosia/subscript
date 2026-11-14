<!-- §296 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 296. A foreign call releases its scratch on each exit

*(Added 2026-11-15.)* Origin: §264.4 item 1. On 2026-11-15 the owner
told Claude to do each item of a list of open runtime defects, in an
order that Claude selects. This item is one of them.

Problem, read at `c3fe474e`. A foreign call with a boundary argument takes a
scratch mark, allocates scratch storage for each such argument
(`subscript_rt_boundary_scratch_alloc`), calls the foreign function,
and releases the scratch to the mark (§187 rule 7, §264). An
allocation can fail. The development tier
(`codegen/src/lower/func/boundary.rs`) checks the trap after each
allocation with `trap_check`, and the ship tier
(`codegen/src/cemit/marshal.rs`) does the same. On that exit, neither
tier releases to the mark. The scratch that the call allocated before
the failure stays until the Context drops. A host that keeps the
Context after the trap keeps that storage.

### 296.1 Rules

1. A foreign call that takes a scratch mark releases the scratch to
   that mark on each exit of the call: the normal exit, the exit of a
   pending trap after the call (§264 rule 2), and the exit of a failed
   scratch allocation before the call.
2. The release runs before the trap check of that exit, as §264 rule 2
   states for the call.
3. A call that takes no scratch mark does not change.
4. The development tier and the ship tier give the same order. No
   program changes its output.
5. The allocation fault of `Context::fail_alloc_after` counts a
   boundary scratch allocation as one allocation request, as it counts
   an object allocation. A fired fault gives the `AllocationFailure`
   trap of the scratch allocation. This makes rule 1 testable; a test
   that uses the fault and a boundary call states the new count.

### 296.2 Acceptance

1. Rule 5 lands first, with a runtime unit test. Then the Red: a test
   makes the second scratch allocation of one foreign call fail with
   rule 5, after the first succeeds, in each tier, and reads the scratch
   count after the trap (`live_boundary_scratch_allocations` in the
   development tier, and the same count in the ship tier). With rule 5
   and no change of rule 1, the count is not 0. The tracking note
   records the counts.
2. After the change, the count is 0 in each tier. A control: the same
   call with no failure gives 0 at the contract commit and after the
   change.
3. No corpus golden changes. The full gate passes.

### 296.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each scratch allocation of both tiers
(two in each), each exit between the mark and the call, and the unwind
block, and found one release on each path. It reverted the two lowering
files on a copy and measured the Red: 1 scratch allocation live in each
tier, against 0 in the working tree.

1. MINOR, contrived, recorded: §296.4 item 1.
2. MINOR, contrived, recorded: §296.4 item 2.
3. MINOR, contrived, recorded: §296.4 item 3.

The first full gate failed in `the_emitted_c_marks_and_writes_back_as_the_plan_says`
(`cli/tests/boundary.rs`). Its C reader walked back from a call to the
scratch mark and stopped at the first scratch release, and rule 1 puts
a release between the mark and the call, inside the failure block of an
allocation. The reader now stops only at a release statement of the
function body.

### 296.4 Open

1. The ship test does not delete its count file before each run. If
   the post-run hook did not run, the test would read the count of the
   run before. The hook runs on each exit today.
2. The test directory stays in the temporary directory if the test
   fails.
3. The new `scratch_mark` fields of the two `BoundaryTargets` structs
   have no doc comment, and one emitted C check has the indent of the
   outer block.
