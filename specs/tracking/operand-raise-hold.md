# Tracking: an operand that can raise holds the operands before it (§295)

Contract: `specs/blocks/compiler/s295-an-operand-that-can-raise-holds-the-operands-before-it.md`.
Origin: §256.4 item 3. On 2026-11-15 the owner told Claude to do each
item of a list of open runtime defects.

## Red

Live allocations of the development tier after `main` of the program
of the problem statement (`codegen/tests/operand_raise.rs`):

| Program | 10 iterations | 50 iterations |
|---|---:|---:|
| contract commit `b406da1a` | 67 | 307 |
| after the change | 47 | 207 |
| control, `const g = gen(); gx(g);`, before and after | 47 | 207 |

The difference to the control is 2 for each iteration: one for
`gx(gen())` and one for `gy(gen(), {})`. Each count grows with the
iterations, because the program runs no collection (invariant 2). So
acceptance item 2 compares against the control, and the contract was
amended before the implementation commit.

The HIR and the LIR have no integer overflow trap. Acceptance item 4
uses the trap of an integer division, and the contract was amended.

## Form

- `codegen/src/lir/array_ownership.rs`: one predicate decides "can
  change or raise" for the lowering. It reads §255 rule 2,
  `Module::expression_can_raise`, and, in the hot reload build, the
  raise edge of a default call.
- `codegen/src/lir/call.rs`, `call_operand_order`: the later operands
  of a call or a `new` in the order of §57.1 (rule 1a). Each hold of
  §255 rule 1 reads it.
- `codegen/tests/support/lir_facts_release.rs` and
  `lir_facts_release_raise.rs`: the release model has its own
  predicate and its own `call_operand_order`, each read from the HIR.
- The LIR text snapshot does not change.

## Phase Review

Two fresh reviews: two MAJOR findings and seven MINOR findings; see
§295.3. The first MAJOR (`new` field initializers) changed the form:
rule 1a. Its measured counts: 35 and 155 before, 25 and 105 after, and
the control 25 and 105.
