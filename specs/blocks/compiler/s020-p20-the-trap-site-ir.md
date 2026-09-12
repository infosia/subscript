<!-- §20 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 20. P20 — the trap-site IR

Owner decision 2026-07-26, after P19's Phase Review. P19 made the two
tiers agree on *call* trap sites by giving them one shared predicate,
`Callee::can_trap()`. About ten **non-call** trap sites were left as
hard-coded policy in each lowering — and **both of P19's own CRITICALs
were instances of that duplication failing**, so this is a demonstrated
hazard, not a tidiness argument.

### 20.1 The property to buy

**It must be impossible to add a trap-capable operation that one tier
checks and the other does not.**

"Impossible", not "tested". P19's §19.7 originally claimed the
`Callee` predicate delivered this "by construction"; the review showed
the claim was broader than the mechanism. The distinction matters:

- a coverage test is **remembering** — it catches the omission only if
  someone extends the test alongside the operation;
- an **exhaustive match over an explicit IR node** is *construction* —
  adding a variant fails to compile in both lowerings, before any test
  runs.

**P20 delivers the second.** A `TrapSite` that either lowering does not
handle is a build error. If the design ends up leaning on a test to
notice an omission, the phase has not met its exit criterion.

**Scope of the guarantee, measured by the Phase Review** (2026-07-26):
it covers a new **variant**, and initially did not cover a new **site
of an existing variant**. Both lowerings select with
`sites.iter().find(|site| matches!(site, TrapSite::X { .. }))`, so an
extra site appended to some operation's sequence compiled in both
tiers and was **silently dropped** by whichever one did not look for
it — leaving §20.5's criterion 2 held by the differential gate rather
than by construction, which is the distinction this section exists to
buy. Only `eval_array_lit` and `eval_template` asserted full
consumption. Closing that is part of the phase; the guarantee is not
"exhaustive match" but **"every derived site is consumed"**, and the
first is only half of the second.

### 20.2 What a trap site has to carry

A boolean cannot express these; each was found in the P19 work.

- **Position.** Each guard reports its own `pos_id`. P19's CRITICAL 2
  diverged in the *trap tuple*, not only in stdout, because the two
  tiers resolved different sites and therefore reported different
  positions.
- **Elision.** §10a lets a proven-in-range index skip its check. The
  set of sites is therefore **decided once, in HIR**
  (`compiler/src/trap_sites.rs`), so both tiers inherit the same
  decision — including §10a's elision of a proven-in-range index, which
  each lowering used to re-derive. That re-derivation was the root of
  the duplication. **This was the substantive part of the phase**: the
  checks a program carries are now a property of the HIR rather than of
  either backend.
- **Multiplicity.** A compound assignment to an array element needs
  **two** resolutions — check-and-read before the RHS, check-and-write
  after — and P19's CRITICAL 2 was exactly one of them being dropped.
  A template literal or an array literal carries several sites. One
  operation therefore maps to a *sequence* of sites, not to a flag.
- **Operands.** A site owns the values its guard tests. P19's
  CRITICAL 1 was an operand string interpolated twice, calling a
  call-valued divisor twice — the guard must hold a materialized
  operand, not a re-emittable expression.

### 20.3 Sites in scope

The non-call sites P19 left in two places: integer div/rem, index read,
index write, `JsonResult.value`, narrowing `as` (null and class
mismatch), the `Context.free` lifetime checks, stale-coroutine,
allocation failure, and the allocation-bearing literals — string
literal, `str_concat`, `fmt_*`, array literal.

*(Corrected 2026-07-26 by the Red stage, which measured rather than
assumed.)* Four of those descriptions were wrong:

- **Narrowing `as` was not "in two places" — the C emitter had no guard
  at all**, only a plain cast (`cemit.rs`, `eval_cast`), whose comment
  claims the path "is not exercised by the run set". C3 says `x as C`
  "traps on `null` or on class mismatch, **in both tiers**". It does
  not, and the Red stage reached it from a corpus program through the
  ambient host-boundary callback types — which is where C7 admits
  `object | null` in the first place, so the path is reachable **by
  design**, not by accident. **This is elevated to the first item of
  stage Green**: it is not a reporting difference but a type-safety
  hole — the ship tier hands back a class-typed reference to an object
  that is not that class, unchecked. P19's review had assessed it as
  unreachable; that assessment was about the run set, not the language.
- **Stale-coroutine exists only under the JIT's reload mode.** The C
  tier has no body-swap mode, so there is no tuple to compare.
- **Allocation failure is not corpus-reachable**: there is no
  source-level memory quota and no allocator fault injection, and
  exhausting real memory is neither safe nor deterministic under
  overcommit. It stays in scope for the IR but cannot be corpus-tested;
  a fault-injection hook is the follow-up if it is wanted.
- **The allocation-bearing list is incomplete.** It omits reference
  class `new` and its constructor unwind, the checker-generated JSON
  `RawNew`, and generator-frame creation — all non-call HIR paths with
  hard-coded checks. Add them.

**A C-only fault point, found by the Red stage and in scope:** every
**non-empty** template in the C emitter emits a checked empty-string
allocation the JIT does not, so the two tiers differ in both the number
and the placement of allocation fault points inside a template. The IR
must give a template the same site sequence on both tiers.

`InvalidDelete` — releasing a pointer the Context does not own — is a
runtime fault this section did not name and which no valid program can
construct today. Recorded so the omission is deliberate.

`Callee::can_trap()` was already shared; it is **folded into
`Expr::trap_sites()`**, so there is one answer to "what can fault
here", not two.

Where a site is **deliberately** checked on one tier only, that must be
representable and stated rather than implicit — Q6's use-after-delete
carve-out (§8.1b) is the existing case, and the review confirmed it is
contracted rather than accidental.

### 20.4 Two pre-existing defects, in scope

Both were found by P19's review, both reproduce before P19, and both
live in the index/compound-assign path this phase restructures.
Fixing them elsewhere would mean touching that path twice.

- `xs[i] += "s"` on a `string[]` emits invalid C — `*p = *p + v` on
  `void*`; there is no `Type::Str` arm.
- `(xs[i] += v)` in expression position fails the ship tier with an
  internal lowering error where the dev tier compiles it.

### 20.5 Corpus and gate (pre-registered)

**Red first.** Before the IR lands, add trap-corpus entries for every
site in §20.3 that has no coverage today, plus accept entries for
§20.4. Entries whose two tiers already agree stay green and are
regression cover; any that diverge are the phase's work.

Exit criteria:

1. **A `TrapSite` variant that a lowering does not handle fails to
   compile.** Demonstrate it — the tracking entry records what the
   build error looks like when a variant is added and one arm is left
   out. This is the phase's reason to exist and the one criterion that
   cannot be waived.
2. The set of emitted checks is derived from HIR, and elision (§10a)
   is decided there. Both tiers emit checks for the same sites on the
   same program.
3. Every §20.3 site has trap-corpus coverage comparing
   `(kind, message, position, pre-fault stdout)` across tiers —
   **except** the three the Red stage showed cannot be compared:
   stale-coroutine (no C mode), the Q6 lifetime checks (§8.1b makes
   the C side unspecified by contract), and allocation failure (not
   reachable without fault injection). Each of those carries an entry
   recording *why* it is not compared, so an unverified site is visible
   rather than absent.
4. §20.4's two defects fixed, with corpus entries.
5. Standing gate green; `tsc` clean; no accept `.expected` moves
   except where §20.4's fixes make a previously-invalid program valid,
   which must be named in the tracking entry.
6. **Benchmarks re-run and recorded.** This phase moves the emission
   of division and indexing again — the paths P19 measured at 82
   instructions against 39. Emitted-C's ratio is reported against
   P19's 1.53×; a regression is a finding, not a cost to absorb.

### 20.6 What landed

**Green, 561 tests, 0 failures.** No pre-existing accept `.expected`
moved; the only additions are `a74` and `a75`, which §20.5 named as the
one permitted reason.

**The criterion that could not be waived is met, and was demonstrated
rather than asserted.** A throwaway `TrapSite::CompileProbe` variant
was added, handled in HIR and in the JIT, and deliberately left out of
the C emitter. The build failed:

```
error[E0004]: non-exhaustive patterns:
  `&subscript_compiler::hir::TrapSite::CompileProbe { .. }` not covered
  --> codegen/src/cemit.rs:927:15
```

Neither lowering's `match site` carries a catch-all arm, so this is
construction and not a test that has to be remembered.

`compiler/src/trap_sites.rs` derives the ordered site sequence in HIR,
elision included. The C tier now traps on both narrowing faults,
closing the type-safety hole the Red stage found. Template allocation
sequences match across tiers, and `a74`/`a75` — the two pre-existing
defects §20.4 pulled in — are fixed with JIT-derived goldens.

**Emitted-C measured 1.53×, unchanged from P19: no regression.** The
`perf-gate` command still exits non-zero because the Cranelift
ship-AOT and dev-JIT thresholds remain missed, which is the
pre-existing §11 situation that motivated C emission and not a P20
finding. Recorded because a non-zero exit that is *expected* should be
written down, or the next reader treats a real failure as routine.
