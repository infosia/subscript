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
| G2 | codegen | local-storage verifier is a copy of the lowering walk | §75.4 | 5 |
| G3 | codegen | fresh-async-owner classification in three tables | §75.1 | 5 |
| G5 | codegen | embedded-boundary-header derived four times | §75.2 | 5 |
| G6 | codegen | boundary-struct-pointer predicate six times, two wrong | §75.3 | 5 |
| C4 | compiler | assignment re-derives the place kind from the lowered `get` call | §76.1 | 6 |
| C5 | compiler | absence test erased to an `Int` sentinel | §76.2 | 6 |
| C6 | compiler | `using` lowered in two passes keyed by `Pos` | §76.3 | 6 |
| R3 | runtime | emitted-check trap messages are copies of the runtime's | §77 rule 1 | 7 |
| R4 | runtime | `===`/SameValueZero written in `arrops` and `assocops` | §77 rule 2 | 7 |

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

## Round 2 — integer literals, enum widening, one HIR walk (landed `ea92d8a`)

Red at `154e221`: `a174` failed in the dev JIT with a Cranelift verifier
error (`arg 1 has type i32, expected i64`); `r170` and `r171` were
accepted and ran.

Fix: `parse_integer_spelling(raw, negate) -> Option<i128>` is the one
reader; enum members are range-checked to `i32`. The enum-to-integer
`Cast` lowered in the C emitter and the interpreter already; Cranelift
did not extend the source. `hir::Expr::children()` and
`hir::Stmt::children()` replace 13 + 13 walks; 1,484 lines removed,
1,142 added.

Measured: `a174` golden on all three tiers; `r170`, `r171` report `S100`
at the initializer (the message text still reads "enum members must
have integer literal values"; a range-specific message is a MINOR for
the next pass). Clippy 7/21/13. Gate on main: 1,125 passed, 0 failed.

## Round 3 — one handle-kind table (landed `85242e9`)

`Type::handle_kind` is the one table; four checker predicates and
`codegen/src/layout.rs` are filters over it. The C emitter's two copies
are deleted.

The first report widened two acceptance filters: `i32[] | null` (S011)
and `===` on arrays and `RegExp` (S100) became accepted. Measured
against main with two probe programs. §74 gained rule 1a (an acceptance
filter keeps the recorded answer; a widening is a corpus decision), and
the correction round restored the answers and pinned both diagnostics
in compiler unit tests. The fact filters keep the runtime's answers:
`AsyncHandle`, `RegExp`, and the `Func | null` box are managed and
dereference a Context allocation.

Candidate widenings recorded in the filters' comments, not decided:
`T[] | null`, `RegExp | null`, `Generator | null`, `AsyncHandle | null`
(S011); identity `===` on `RegExp`, `object`, arrays, generators, async
handles, `Worker`, `Inbox`, `Outbox` (S100).

Measured: `Worker`, `Inbox`, `Outbox`, bare `Func` not managed (§74
rule 3 record). Clippy 7/21/13. Gate on main: 1,131 passed, 0 failed.

## Round 4 — the terminator walks itself (landed `e3b9d4f`)

`Terminator::targets()`, `successors()`, `value_uses()`, `map_values()`
replace eleven walks; 607 lines removed, 481 added. Reads versus
mentions: liveness, address escape, and C declaration references read;
value replacement, unroll's external-use check, the root plan's
interference, and copy elimination mention (each with a one-line reason
at the site). Clippy 7/21/13. Gate on main: 1,133 passed, 0 failed.

The debug-profile interpreter ledger (`DEBUG_INTERPRETER_SUBSET`,
`DEBUG_RUNNABLE_COUNT`) had not gained `a173` and `a174`; added with
this round's record. The inventory list of round 1 gains that ledger.

## Round 5 — four LIR facts (landed `f5bb47d`)

Fresh-owner bit on the LIR value from one instruction table (function
parameters are not fresh owners; §70.3 rules 1–2). `is_embedded_header`
set once. One `boundary_box_class`; the two one-condition sites
differed on `T | null` with `T` a boundary reference class (old 1695)
and with `T` a non-boundary value class (old 5029); the 173-entry trace
reaches neither input — a corpus entry for each shape is open (core
principle 12). `verify_local_storage_classes` deleted; the interpreter
poisons Activation locals at every Suspend. 443 lines removed, 327
added. Clippy 7/21/13. Gate on main: 1,135 passed, 0 failed.

## Round 6 — three checker facts (landed `1224881`)

`Place` classified before member lowering (seven variants);
`ExprKind::AbsenceTest` replaces the `Int` sentinel and `narrow_paths`
loses its alias closure; `using` is `Stmt::Let { dispose: true }` with
one scope-exit pass (195 lines removed, 122 added in `check/mod.rs`).
The round stopped once: the `dispose` field needed pattern updates in
`codegen/src/lir.rs`, outside the handoff's file list; the list was
widened. Clippy 7/21/13. Gate on main: 1,137 passed, 0 failed.

## Round 7 — two runtime facts (landed `479676b`)

`TrapKind::message` owns every trap message; two kinds had two
spellings. `runtime/src/valeq.rs` owns `value_eq` and `read_uint`;
`F16` is array-only (§10.2 rejects it as a key; the `KeyKind` ABI has
no `F16` tag). Clippy 7/21/13. Gate on main: 1,140 passed, 0 failed.

## MAJOR pass: state

All 16 MAJOR findings landed in seven rounds (`5487643`..`479676b`).
The MINORs (66) are not started. A Phase Review of the cumulative diff
follows.
