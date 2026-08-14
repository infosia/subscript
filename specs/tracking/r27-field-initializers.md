# R27 — field initializers run on every construction

Status: **landed 2026-08-15** against `specs/blocks/compiler.md`
§57. Origin: downstream request R27. Contract `d1c17fc`,
implementation `6026342`.

## The request

A `@CStruct` class with no constructor and a field initializer
prints the initialized value on the dev tier and zero on the ship
tier. No diagnostic appears and no trap fires. The downstream
avoids the shape with a generator rule and escalated the silent
divergence. Ask: decide which tier is correct, make the other
match, and pin the answer with a corpus fixture.

## Findings on this host, at `b1a5dab`

- Reproduced the report. The reference-class shape diverges the
  same way: dev prints the value, ship prints zero.
- With a constructor, a side-effecting initializer and a
  side-effecting argument order differently: dev runs the
  initializer first, ship runs the argument first. Under `node`
  the same source runs the argument first (measured, exit 0).
- The checker accepts `this` in a field initializer. The dev tier
  then fails with `internal lowering error`, with or without a
  constructor. No program with that shape runs on the dev tier.
- Causes: the C emitter ran initializers only inside the emitted
  constructor; the Cranelift lowering ran them before the
  constructor arguments; the checker gave the initializer context
  a `this` binding that no tier lowers.

## What landed

Both tiers observe one order: arguments evaluate left to right,
the construction zero-initializes the instance, the declared
initializers run in declaration order once per construction, then
the declared constructor body runs. The C emitter runs
initializers for constructor-less value and reference `new`; the
Cranelift lowering evaluates constructor arguments first. The
checker checks a field initializer with no `this` binding, so
`this` there fails with S100. `collisions.md` gains C9.

Corpus: `a133-field-init-no-ctor` and `a134-field-init-order`
(accept, with goldens; a134 pins `arg runs` before `init runs`),
`r126-this-in-field-init` (S100).

## Red, at the contract pin

- `a133`: dev `value:37` / `reference:41`; ship `value:0` /
  `reference:0`.
- `a134`: dev ran `init runs` before `arg runs`; ship the
  reverse.
- `r126`: the checker accepted it.

## Gates (this host, at `6026342`)

- `cargo test --offline --workspace`: 55 suites, 935 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden and `.expected` file is
  byte-identical; the only new goldens are a133's and a134's
  (134 total).
