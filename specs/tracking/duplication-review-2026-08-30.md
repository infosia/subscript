# Duplication and over-complexity review — 2026-08-30

Owner decision, 2026-08-30: fix every MAJOR; decide on the MINORs after.

## Scope and method

Three fresh reviewers, one per crate (`codegen/`, `compiler/`,
`runtime/`), read-only, at `f99d4cb`. Two classes: (A) one computation
or decision table written in two or more places; (B) a mechanism larger
than the problem. The three transcribers' independence (§68) is by
design and was excluded.

Counts: codegen MAJOR 6 / MINOR 17; compiler MAJOR 6 / MINOR 21;
runtime MAJOR 4 / MINOR 28.

## The MAJOR findings and their contracts

| # | Crate | Finding | Contract | Round |
|---|---|---|---|---|
| R1 | runtime | `map`/`filter` did not root the result across callbacks; the fixed family did | §8.1e rule 1 | 1 |
| R2 | runtime | per-class release work on three free paths | §8.1e rule 2 | 1 |
| C2 | compiler | three integer-literal readers; enum reader read the `f64` value | §72.1 | 2 |
| — | compiler/codegen | `enum as i64` accepted and not lowered (found by the C2 probe) | §72.1 rule 3 | 2 |
| C3 | compiler | 13 + 13 hand-written HIR walks; two drop subtrees | §72.2 | 2 |
| C1 | compiler | four handle-type tables, drifted | §74 rule 1 | 3 |
| G1 | codegen | managed-type table in `layout.rs` and `cemit.rs`, drifted | §74 rules 2–3 | 3 |
| G4 | codegen | eleven Terminator walks; `invalidates` counted as a use in three | §73 | 4 |
| G2 | codegen | local-storage verifier is a copy of the lowering walk | pending | 5 |
| G3 | codegen | fresh-async-owner classification in three tables | pending | 5 |
| G5 | codegen | embedded-boundary-header derived four times | pending | 5 |
| G6 | codegen | boundary-struct-pointer predicate six times, two wrong | pending | 5 |
| C4 | compiler | assignment re-derives the place kind from the lowered `get` call | pending | 6 |
| C5 | compiler | absence test erased to an `Int` sentinel | pending | 6 |
| C6 | compiler | `using` lowered in two passes keyed by `Pos` | pending | 6 |
| R3 | runtime | emitted-check trap messages are copies of the runtime's | pending | 7 |
| R4 | runtime | `===`/SameValueZero written in `arrops` and `assocops` | pending | 7 |

## Round 1 — runtime rooting and release (landed `f0cc4ed`)

Red: `a173` trapped `[internal]: array storage disappeared while
growing it` at `5487643` on the dev JIT, through a function-value
callback (a known callback takes the §8.1d loop and never reached the
runtime).

Fix: one loop per operation over an element source; `map` and `filter`
root the result across the callbacks. `Context::release_class_state`
replaces three copies; a unit test frees each class on each of the
three paths.

Measured: `a173` golden on all three tiers. Runtime 253 passed. Clippy
7/21/13. Gate on main: 1118 passed; the 3 failures are the round-2 Red
entries (`a174`, `r170`, `r171`).

The round stopped once on the corpus inventory assertions
(`golden.rs`, `corpus_accept.rs`, `corpus_reject.rs`, `corpus_warn.rs`,
`js_corpus.rs`, `lir.rs`, `generated-docs/`), which the handoff did
not name. Rule: a corpus addition updates every inventory assertion and
regenerates `generated-docs/` in the same commit as the entry.
