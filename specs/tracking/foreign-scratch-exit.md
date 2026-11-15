# Tracking: a foreign call releases its scratch on each exit (§296)

Contract: `specs/blocks/compiler/s296-a-foreign-call-releases-its-scratch-on-each-exit.md`.
Origin: §264.4 item 1. On 2026-11-15 the owner told Claude to do each
item of a list of open runtime defects.

## Red

Rule 5 landed first: `boundary_scratch_alloc` reads the one-shot fault
of `fail_alloc_after`. No existing test that uses the fault changed its
count.

With rule 5 and no change of rule 1, a foreign call whose second
scratch allocation fails left 1 scratch allocation live after the trap,
in the development tier and in the ship tier. After the change, each
tier gives 0. The control with no failure gives 0 before and after.

## Form

- `runtime/src/context/lifecycle.rs`: the scratch allocation reads the
  fault.
- `codegen/src/lower/func/boundary.rs` (development tier) and
  `codegen/src/cemit/marshal.rs`, `codegen/src/cemit/call.rs` (ship
  tier): a failed scratch allocation releases to the mark of the call
  before the trap check.
- Tests: `runtime/src/context/tests_0.rs` (rule 5) and
  `codegen/src/reload/scratch_exit_tests.rs` (both tiers).

## Phase Review

One fresh review: no CRITICAL or MAJOR finding, three contrived MINOR
findings, recorded as §296.4 items 1 to 3. The review measured the Red
on a copy: 1 live scratch allocation in each tier before the lowering
change, 0 after. The pair of tier tests costs 0.82 to 0.85 s.

The first full gate failed in the C reader of `cli/tests/boundary.rs`:
the failure release of rule 1 stopped its walk back to the scratch
mark. The reader now passes a release inside a failure block, and its
firing control holds that form.
