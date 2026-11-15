# Tracking: a type argument is inferred from the narrowed argument (§301, withdrawn)

Contract: `specs/blocks/compiler/s301-a-type-argument-is-inferred-from-the-narrowed-argument.md`.
Origin: §252.4 item 1. On 2026-11-15 the owner selected the item, and
withdrew it the same day on the measurement below.

## Red

At the contract commit `91f712f4`, `subscript check` of the form
`const g: CB | null = null; two(g, (x) => print(\`${x}\`));` gave
"no errors". `tsc` 5.9.2 gives TS2345 and TS7006.

## Measurement

- Round 1: the flow analysis has no fact "is `null`"
  (`compiler/src/check/narrowing_fact.rs`, `FactKind`), and the
  registration of an initializer fact excludes `Type::Null`
  (`compiler/src/check/stmt.rs`). The round stopped (core principle 8).
- Round 2, with a fact `IsNull` and the read type `null`: the form gave
  S100 "parameters require a type annotation" at the lambda. `id(g)`,
  which `tsc` accepts, gave S100 "cannot infer type parameter T of id:
  no candidate" (`join_candidates` drops a candidate that is only
  `null`). `if (g !== null) { g(1); }` stopped the checker at an
  internal assertion of two fact kinds. `r426` changed its S012 text.
- Each change was reverted. No code of §301 landed.
