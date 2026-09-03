# A generator consumed only through `for…of` did not lower

Status: **landed 2026-09-03** against `specs/blocks/compiler.md` §83.
Origin: review round 3 of §82 (R39) found it outside that diff.
Contract `7f46a3a`, amended `90be0c2` after the review; implementation
`35fe70e`, fixes `efdc31d` and `db3449d`; contract amended `90be0c2`
and `9801490`.

## The defect

At `088acac`, `for (const v of values())` with no spelled `.next()`
checked clean and stopped at lowering on both tiers with "call
disagrees with the signature table: BuiltinMethod.GeneratorNext
declares , got [Data(Generator(I32))] -> Some(Data(IterResult(I32)))".
`a79` passed because it also spells `generator.next()`.

Cause: `hir::Module.operation_signatures` was filled by
`register_operation_signature` as a side effect of `check_expr`. The
`next` call that `check_for_of` synthesizes never passed through it.
Eight other sites registered synthesized nodes by hand.

## What landed

- One walk over the finished HIR derives the table after the check
  (`compiler/src/check/mod.rs`); the side-effect registration, its
  `RefCell`, and its eight call sites are deleted; one callee-to-target
  mapping in `compiler/src/hir.rs` serves the walk and the lookup.
- The walk also found `Ambient(Unreachable)` entries that three
  entries lacked (a116, a162, a163): a second synthesized-call site
  the side effect missed, and no failure had shown it because the
  LIR lowering of `unreachable()` did not consult the table.
- HIR JSON: entries added in a116, a162, a163; order changed with no
  addition in a136, a146, a69, a70, a71, a72, a79; no other field
  changed.
- Corpus: `a180-for-of-generator-only` (full run, `break`,
  `continue`, a `@CStruct` element; `js-comparable: no C2`). Red at
  `7f46a3a`, exit 2, quoted above. Tests:
  `compiler/tests/operation_signatures.rs` (hand-written expected
  tables for a180 and a181; a total check over accept, warn, and
  examples with a positive control that inserts a synthesized call;
  an iterator test with a hand count).
- Counts: accept `.ts` 177 → 179; `.expected` 178 → 180 (a180, a181).

## Gates (this host, `db3449d`)

- debug (the coding agent's final run): 66 suites, 1,243 passed, 0
  failed, 1 ignored, 1,962 s.
- release: 66 suites, 1,241 passed, 0 failed, 1 ignored, 340 s.
- At `35fe70e` the release run showed 3 failures in `codegen/tests/lir.rs`
  ("no source files given" for an entry `subscript-build`): a
  reviewer's `subscript build --source corpus/accept/a180…` had written
  its output directory beside the source, and the harness enumerated
  it. Removed; 35/35.
- Zero-warning build; fmt, `tsc`, hygiene exit 0; clippy 7 / 18 / 13.
- No pre-existing golden or `.expected` moved.

## Review round 1 (fresh no-context subagent)

§83.4 holds the record. CRITICAL: the walk listed owners by hand and
skipped parameter defaults — `factor: f64 = Math.max(2.0, 3.0)`
lowered at `7f46a3a` and failed at `35fe70e` (rule 1: one owner
iterator on `hir::Module`, shared with `trap_sites`); the two tests
re-derived the table with copies of the production code and could
not fail (items 3–5: hand-written expected tables, a positive
control that inserts a synthesized call, an iterator test with a
hand count). MAJOR: no positive control; a third copy of the callee
mapping in `codegen/src/lir.rs` (rule 3). MINOR: the `Arr::Map` /
`Filter` `ArrayPush` append (rule 4 states it); the examples scope;
no HIR serializer, so item 6 records the measured JSON differences;
a copied interop token list; order-sensitive lookup; the untracked
note; `subscript build` writes beside its source. Fixed in
`efdc31d`: a181 pins one operation call per owner kind (Red at
`35fe70e` for the parameter-default forms: "Math.Max declares ,").

## Review round 2

§83.5 holds the record. MAJOR: the LIR execution-fact helper had
gained an `if id == "a181…"` exception (item 6: it resolves a method's
parameter count as it does for a function and a constructor); the
iterator was `&mut self` only, so `warn.rs` and the test support kept
hand lists (rule 1: both borrow forms; `check/layout.rs` and the
lowering are the two exempt, identity-carrying passes). MINOR: rule 1
wording on lambdas; an unreachable `foreign_fns` arm; the
lambda-default arm changes no accepted program (form-total, stated);
the total test shares the iterator (controls stated); the iterator
test counts calls; three interop tokens; a `next` match at the reload
trap site (outside rule 3); the note's "independent walk" wording; a
stale comment. Fixed in `db3449d`: debug 66 suites, 1,243 passed, 0
failed, 1 ignored, 1,962 s; clippy 7 / 18 / 13.

## Recorded, not changed

The dev command reports an internal lowering failure under "internal
lowering error:" and the ship command under "internal error:"
(§83.3). No program that checks clean reaches either text now.
