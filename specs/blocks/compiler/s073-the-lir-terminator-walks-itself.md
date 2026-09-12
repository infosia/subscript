<!-- §73 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 73. The LIR terminator walks itself

*(Owner decision, 2026-08-30; review finding.)*

**Rule 1.** `Terminator` in `compiler/src/lir.rs` exposes three methods,
and no consumer writes its own `match` over `Terminator` to find edges
or values:

- `targets()` — every `BlockTarget` in order (branch, both conditional
  arms, switch arms then default, suspend successor with its
  arguments);
- `successors()` — the block ids of `targets()`;
- `value_uses()` — every value the terminator reads: the condition, the
  switch value, the return value, every target argument, the suspend
  kind's operands (`AsyncCall` operands, `AsyncHandle` handle), and the
  suspend `resume_value` is not a read.

**Rule 2.** `Suspend.invalidates` is not a read. It names the arrays
whose storage can move across the suspension (§68.2 item 9). A consumer
that needs "mentions" (reads plus invalidations) adds `invalidates`
itself, and says why at the site.

Measured before the rules: eleven hand-written walks in `codegen/`
(`lir.rs`, `lir/unroll.rs`, `root_storage.rs`, `cemit.rs`). Three
counted `invalidates` as a use; the liveness walk `lir_live_ins` and
`verify_suspend_definition_boundaries` did not. The C emitter and the
liveness computation disagreed on what a suspend uses.

**Check.** A unit test builds one terminator of every kind with distinct
values in every operand position and checks that `value_uses()` yields
each one once and that `successors()` yields every target. The fact
check (`lir_facts.rs`) reports any `match` site over `Terminator` outside
`compiler/src/lir.rs` that reads targets or operands — enforced by
review, not by the build.
