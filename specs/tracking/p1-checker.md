# P1 — semantic checker + typed HIR: evidence

Status: COMPLETE, 2026-07-22. Contract: `specs/blocks/compiler.md` §6.

## Gate evidence (orchestrator-verified, independent run, post-review)

- `cargo build --offline`: zero warnings.
- `cargo test --offline`: 69 passing (59 unit + 8 integration in
  `compiler/`, 2 spike), zero failures.
- Reject gate: all 14 entries rejected with the §6-contracted code at
  the offending line (r02 and r05 both S002); the test table covers the
  directory exhaustively.
- Accept gate: all 24 entries (a19 as one two-file program) check clean
  and produce typed HIR; resolved-type spot asserts on a02 conversions,
  a04 value class, a12 monomorphized instances, a17 narrowing, a20
  `Generator<i32>`.
- Library code has zero panic sites (unwrap/expect/panic!/unreachable!
  all confined to test modules).

## Implementation decisions (recorded per handoff; binding until revised)

- **Monomorphization at check time, in HIR**: explicit type arguments
  instantiate templates on first use; generic templates do not survive
  into the module. Consequence: a never-instantiated generic has an
  unchecked body.
- **Code assignments where §6 named no rule**: mixed-type arithmetic and
  mixed-width bitwise (Q18) → S007; member access on non-narrowed
  `Ref | null` → S011; unknown member writes → S004, reads → S100;
  arity/unknown-name/`instanceof`/loose-`==`/`var`/destructuring/
  FixedArray-length/const-rebinding → S100.
- **C4 context extends to binary operands** (a22's `lcgState * 1664525`);
  `as` targets are not a literal context.
- **Local inference**: un-annotated locals infer from initializers;
  module-level variables require annotations.
- **C5 escape set**: return, store to global/field/array element,
  constructor arguments; locals holding capturing lambdas are tainted.
- **Narrowing**: facts killed on assignment and (conservatively) for
  names assigned anywhere in a loop body; calls do not invalidate —
  sufficient for the corpus, flagged for P2+ refinement.
- **Pins**: SWC `swc_common =5.0.1` / `swc_ecma_ast =5.1.0` /
  `swc_ecma_parser =6.0.2`; direct `serde =1.0.219` (newer serde 1.x
  removed `serde::__private`, which `swc_common 5.0.1` imports).

## Phase Review (2026-07-22)

Fresh no-context review with execution-verified adversarial probes:
0 CRITICAL, 3 MAJOR, 7 MINOR. Fixed (each with a regression test;
13 added):

- MAJOR: enum implicit-value i64 overflow panicked → checked arithmetic,
  S008.
- MAJOR: three C5 escape routes accepted capturing lambdas (`push`
  argument; array literals in inferred/FixedArray contexts plus tainted
  locals; conditional expressions in `return`) → all S009;
  `is_capturing_value` now forwards taint through Cond/Assign/ArrayLit,
  with the remaining-expression audit documented on the function.
- MAJOR: no all-paths-return analysis (non-void function could fall off
  the end, producing ill-formed HIR for P2) → conservative
  `always_returns` analysis, S100, generators/constructors exempt;
  applies to lambda block bodies too.
- MINOR fixed: `++`/`--` on `const` bindings (Q17, S100); user-written
  `object` annotations rejected as boundary-only (C7, S011); cross-file
  duplicate class names diagnosed (S100); `FixedArray` length beyond
  u32 range rejected (S008) instead of saturating.

Recorded as deliberate, not fixed:

- Narrowing facts survive calls (matches `tsc`; runtime null safety is
  P2's trap model — revisit there).
- `3000000000 as u32` is S008 (false reject): consequence of the
  recorded "`as` is not a literal context" decision; the annotated
  declaration form is the supported spelling.
- Internal `ClassId`-style indexing can panic only on internal-invariant
  breach (not input-reachable; audited during review).

## P1 exit

Gate met post-fix: 14/14 rejects at contracted (code, line), 24/24
accepts clean with typed HIR, zero open CRITICAL/MAJOR. Next: P2 —
runtime + HIR→CLIF + JIT; goldens per compiler block §2.
