# Tracking: a statement place drops each value leaf (§300)

Contract: `specs/blocks/compiler/s300-a-statement-place-drops-each-value-leaf.md`.
Origin: §258.4 item 1. On 2026-11-15 the owner selected the item.

## Red

At the contract commit `2c0af5a5`, `subscript check` of `a459` exited 1.
Each form of the contract table gave its S100: "logical operators
require booleans, got `i32`" at each `&&` and `||` right operand, and
"conditional branches have no common type: `i32` and `string`" at
`b ? 1 : "s";`.

## Form

- `compiler/src/check/expr/void_leaves.rs`: the statement place of
  §258 gives each branch and each right operand of `&&` and `||` the
  statement place, with no `void` leaf needed. The branches take no
  common type and no literal context from each other there.
- `compiler/src/check/expr/operator.rs`, `conditional.rs`: the right
  operand of `&&` and `||` in a statement place is not a truth test.
  The left operand and the condition keep C24 row 1.
- A statement conditional lowers to `Stmt::If`, so two HIR tests of
  `x ??= make();` (§199) compare the value position now; the `assert_ne`
  control stays.
- No codegen change. The LIR text snapshot does not change.
- `specs/blocks/collisions.md`: C21 cites §300 and `a459`.

## Phase Review

One fresh review: no CRITICAL or MAJOR finding, two MINOR findings,
each fixed; see §300.3. The review measured release parity on the
development tier: over 300,000 iterations, the maximum RSS of the
statement form equals the `if` form for an array, a string, a class,
and a generator leaf.
