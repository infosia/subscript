# P20 — the trap-site IR. COMPLETE 2026-07-26

Contract: `specs/blocks/compiler.md` §20. Opened straight out of P19's
Phase Review: P19 gave the two lowerings one shared predicate for
*call* fault sites and left about ten **non-call** sites hard-coded
separately in each, and **both of P19's own CRITICALs were instances of
that duplication failing**.

## The criterion, and the proof

§20.1 asked for one thing that could not be waived: adding a
trap-capable operation that one tier checks and the other does not must
**fail to compile**, not fail a test. A coverage test is remembering;
an exhaustive match over an explicit IR node is construction.

Demonstrated rather than asserted. A throwaway variant, handled in HIR
and the JIT and deliberately omitted from the C emitter:

```
error[E0004]: non-exhaustive patterns:
  `&…::TrapSite::ReviewProbe { .. }` not covered
   --> codegen/src/cemit.rs:927:15
   --> codegen/src/lower/func.rs:596:15
```

Two errors, one per lowering. Neither `match site` carries a catch-all.
The Phase Review reproduced this independently in its own scratch tree.

## The guarantee was half of what §20.1 claimed

The review measured its scope and found it covered a new **variant**
but not a new **site of an existing variant**: both lowerings selected
with `sites.iter().find(|site| matches!(…))`, so an extra site appended
to an operation's sequence compiled in both tiers and was **silently
dropped** by whichever one did not look for it. Only `eval_array_lit`
and `eval_template` asserted full consumption.

That left §20.5's criterion 2 resting on the differential gate — the
exact thing §20.1 was written to replace.

Closed by tracking each operation's sites in `codegen/src/trap_sites.rs`
and wrapping **every** consumer in both lowerings with the same check.
No consumer turned out to be unable to assert it. Demonstrated with two
`DivisionByZero` sites where only one is consumed:

```
integer division has unused HIR trap sites:
  DivisionByZero { pos: Pos { file: "probe.ts", line: 3, col: 7 } }
```

**The guarantee is "every derived site is consumed", not "the match is
exhaustive". The second is half of the first**, and the phase was not
finished until both held.

## What else landed

- `compiler/src/trap_sites.rs` derives the ordered site sequence **in
  HIR**, elision included, so §10a's proven-in-range decision is made
  once and both tiers inherit it. Each lowering used to re-derive it,
  which was the root cause. The review confirmed `checked` defaults to
  `true` at every construction site and `decide_index_checks` only ever
  clears it — a missed traversal fails safe.
- **The C tier's narrowing hole is closed.** C3 promised `x as C` traps
  on `null` or class mismatch "in both tiers"; the C emitter emitted a
  plain cast, handing back a class-typed reference to an object that
  was not that class. The review verified the pre-state (ship-C ran to
  completion where the JIT trapped) and that the guarded path is now
  the only one reaching `Cast`.
- **Template site sequences match.** The C emitter had been emitting a
  checked empty-string allocation per non-empty template that the JIT
  did not — 9 fault points against 7 for `` `x${a}y${a}` ``.
- `a74`, `a75` and `a76` fix three C-emitter defects, all pre-existing.
  The third — a field write through a value-class element of a dynamic
  array (`xs[1].x = 9`) — was found by the review, not by the contract:
  same failure shape and same error string as §20.4's second item, in
  the function P20 rewrote, so §20.4's own rationale applied.
- `t26` records **why** allocation failure is not compared across
  tiers, as a non-runnable policy entry with a deliberately empty
  golden. §20.5 required each uncomparable site to say why it is
  unverified rather than be silently absent; this was the one missing.

## A cross-tier divergence P20 fixed without noticing

Found by the review and confirmed by re-running the pre-P20 tree.
`get(b).value /= zero`:

```
95c3e81~1   dev-JIT 9:3      ship-C 9:10
HEAD        dev-JIT 9:10     ship-C 9:10
```

A real trap-tuple divergence no golden covered. The **dev tier moved to
the C tier's answer** — `target.pos` rather than the assignment
expression's. Recorded because a fix nobody set out to make is exactly
the kind that gets reverted later by someone who does not know it was
one.

## Performance — read, not benchmarked

The review dumped and compiled emitted C for an indexing-heavy and a
division-heavy unit at `-O2` on both trees. Division: no change.
Indexing: one extra no-op pointer-cast temporary, and the `.s` output
is **byte-identical** apart from two `pos_id` immediates. Instruction
counts equal per function — 89/89, 76/76, 51/51, 26/26.

The site machinery added **no per-operation ship-tier cost**, which is
what makes §20.6's "1.53×, unchanged from P19" credible rather than
merely reported. The dev-JIT side was not measured.

## Phase Review — 0 CRITICAL, 1 MAJOR, 5 MINOR. All closed.

40 hand-written programs across multiplicity, operands, position and
elision; every non-trapping program byte-identical across tiers, every
trapping program identical in `(kind, message, position, pre-fault
stdout)`, with the one MAJOR above as the exception. **No surviving
double evaluation** — the failure mode of P19's CRITICAL 1 — was found
anywhere a guard tests a value.

MINORs: this tracking file was missing; allocation failure had no
policy record; six clippy warnings were added and are now back to the
16-warning baseline, the three unavoidable `too_many_arguments`
carrying justifying comments rather than bare allows.

`TrapSite` deliberately omits `#[non_exhaustive]`, against the CLAUDE.md
convention, because the phase's whole mechanism depends on
cross-crate exhaustiveness. Justified in the type's doc comment; the
review confirmed this is correct as written.

## Gate

`cargo build --offline --all-targets` zero warnings; `cargo test
--offline` **562 passed, 0 failed**; `tsc` exit 0; `git diff --check`
clean; `cargo clippy -p subscript-codegen` 16 warnings, the pre-P20
baseline. No pre-existing accept `.expected` moved — the only additions
are `a74`, `a75` and `a76`, which §20.5 named as the one permitted
reason.

`perf-gate` exits non-zero on the Cranelift ship-AOT and dev-JIT
thresholds. That is the pre-existing §11 situation that motivated C
emission, not a P20 finding — written down so a future reader does not
mistake a real failure for this routine one.

## Carried forward

Allocation failure cannot be corpus-tested without **allocator fault
injection**; `t26` records the gap. A hook is the follow-up if the
site is to be verified rather than only represented.
