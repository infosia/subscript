# §87 — a synthetic owner is one scoped operation

Status: **in progress.** Contract: `specs/blocks/compiler.md` §87
(`74bc806`; amended `6a89c9a`). Origin:
`specs/tracking/development-cost-review-2026-09-05.md` finding 1.

## Round 1 (at `3967e9c`)

One operation `with_synthetic_owner(kind, body)` replaces the
enter/push/drain/finish quartet at the 13 sites; seven kinds; a
typed `SyntheticPrefix`; a 14-cell matrix test with literal
expectations read from the HIR. `cargo test -p subscript-compiler`
424 passed; the release golden and LIR sweeps passed; no golden
moved; clippy 7.

Fresh review (read-only): CRITICAL 0, MAJOR 2, MINOR 8.

- MAJOR, a form defect: a `let`/`const`/`using` with two or more
  declarators had the statement as its owner, and the round placed
  every declarator's prefix before the first binding —
  `let a: Box = new Box(2), b: i32 = (pick(a) ?? fb).v;` ran
  `pick(a)` before `new Box(2)`. The committed checker drained per
  declarator; §82.10's owner list did not say so; no corpus entry
  had the shape, so the gate could not see it. Contract: the
  `Declarator` owner (§87.1 rule 2, §82.10 rule 1), and a183 pins
  the order. Forced.
- MAJOR, core principle 9: in the scoped form the operation drains
  and pops on every path, so the §82.10 rule 3 escape check read a
  collection the operation had just emptied and had no input that
  could fire it. Contract: the drain is the boundary; the S100
  report is deleted (§87.1 rule 3). Forced.
- MINOR, fixed in round 2: a default in the result trait let a
  rejecting kind with a non-expression body misreport; a
  test-only trait impl in library code; no `#[must_use]` on the
  operation and the prefix; no `///` on the operation; drain and
  leave still callable from the sibling modules; the switch-case
  site left at an indentation rustfmt bails on.
- MINOR, recorded: `push_synthetic_prefix` keeps an `expect` in
  library code (pre-existing); a body that panics does not pop (no
  unwind guard; unreachable by core principle 5).
- Outside the diff, same class, **open**: a `while` condition is not
  an owner, so `while ((maybe() ?? fb).v > 0)` calls `maybe()` once.
  §82.10 rule 3a records it; it needs its own request and corpus
  entry.

The first full gate on the round-1 tree was stopped at the review's
MAJOR; no partial record remains.

## Round 2, first stop (at `6a89c9a`)

The coding agent measured the a183 shape on the round-1 tree before
any fix: `new Box 1`, `new Box 2`, `pick 2`, `1` on the dev JIT, and
the same bytes under `node`. The review's runtime claim was wrong:
the synthetic `Let` of §82.3 carries a `Null` initializer and the
receiver call stays inside the expression, so the misplaced `Let`
changes the HIR and not the order of effects. Core principle 10: an
entry that never failed proves nothing, so a183 is dropped and the
`Declarator` rule stands as a placement rule whose Red is the matrix
cell (`ad5b438`). Found by the coding agent's Red measurement, as
the handoff required.

## Round 2 (at `ad5b438`)

Red: with the two `Declarator` matrix cells added and no other
change, the matrix failed with
`left: ["synthetic Let", "a", "b"]  right: ["a", "synthetic Let", "b"]`.
Green after the `Declarator` owner in `check_bindings`. The dead
escape check and its S100 text are deleted; push and pop are one
operation; `#[must_use]` and `///` on the operation and the prefix;
the switch-case test body is a method rustfmt formats. 16 cells
green; `cargo test -p subscript-compiler` 424; release golden and
LIR sweeps green; clippy 7; workspace build 0 warnings.

Second fresh review: CRITICAL 0, MAJOR 0, MINOR 2 — three `///`
lines over 100 columns (`mod.rs` 524, 585, 586; rustfmt does not
police comments without a `rustfmt.toml`); the one operation still
admits a non-expression body under a rejecting kind (the `bool` and
`Vec<hir::Stmt>` result impls answer `None`), which the contract
permits — a second operation typed for the two rejecting kinds is a
contract option for the owner. Both recorded, not fixed in this
round. The reviewer confirmed the single-declarator path
byte-identical to HEAD and the diagnostic order unchanged.
