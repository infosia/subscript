# Tracking: an invalidation list names only values that dominate it (§297)

Contract: `specs/blocks/compiler/s297-an-invalidation-list-names-only-values-that-dominate-it.md`.
Origin: §273.4 item 1. On 2026-11-15 the owner told Claude to do each
item of a list of open runtime defects.

## Red

With rule 3 only (the verifier check), the lowering of the corpus gave
rejections in 86 entries, not in `a341` alone. The rejection of
`a341` reads:

```text
function 5 (`arrays`): use of value 27 in block 10 is not dominated by its definition
```

Cause, found in the lowering: each instruction that can invalidate
array addresses took a copy of the array list of the whole function.
So the list held values of other branches, values defined later, and
the result of the instruction itself.

## Form

- `codegen/src/lir/invalidation.rs`: the list of an instruction holds
  only the array values that are in scope at the instruction (rule 2).
- `codegen/src/lir.rs`: the scope pass runs before the SSA construction
  of §273.
- `codegen/src/lir/verify_dominance.rs`: the verifier rejects a list
  value that does not dominate its instruction (rule 3), with a unit
  test of a value from another branch and a dominating control.
- The LIR text snapshot changes in 222 lines. Each changed line is an
  invalidation list; a list that became empty drops its
  `invalidates=` field. No corpus golden changes.

## Phase Review

One fresh review: no CRITICAL or MAJOR finding, three MINOR findings,
each fixed; see §297.3.
