# A generator consumed only through `for…of` did not lower

Status: **landed 2026-09-03** against `specs/blocks/compiler.md` §83.
Origin: review round 3 of §82 (R39) found it outside that diff.
Contract `7f46a3a`, implementation `35fe70e`.

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
  `compiler/tests/operation_signatures.rs` (an independent second
  walk equals the table; a total check over accept, warn, and
  examples).
- Counts: accept `.ts` 177 → 178; `.expected` 178 → 179.

## Gates (this host, `35fe70e`)

- debug: 66 suites, 1,240 passed, 0 failed, 1 ignored, 1,625 s.
- release: 66 suites, 1,238 passed, 0 failed, 1 ignored, 334 s. One
  run showed 3 failures in `codegen/tests/lir.rs` ("no source files
  given" for an entry `subscript-build`): a reviewer's `subscript
  build --source corpus/accept/a180…` had written its output directory
  beside the source, and the harness enumerated it. Removed; the suite
  passes 35/35.
- Zero-warning build; fmt, `tsc`, hygiene exit 0; clippy 7 / 18 / 13.
- No pre-existing golden or `.expected` moved.

## Review

REVIEW_TODO

## Recorded, not changed

The dev command reports an internal lowering failure under "internal
lowering error:" and the ship command under "internal error:"
(§83.3). No program that checks clean reaches either text now.
