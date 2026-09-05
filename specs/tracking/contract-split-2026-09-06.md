# The contract split and the pipeline text — 2026-09-06

Origin: `development-cost-review-2026-09-05.md` finding 6.

## What changed

- `specs/blocks/compiler.md` opens with a status paragraph, §0 (a
  stage → sections map and a total section index with a status per
  section), and a current §1 diagram (parse → checker → HIR → LIR,
  verified → the dev JIT, the C emitter, the reference interpreter).
- `specs/blocks/compiler-history.md` (new) holds the revision log
  (Rev 0–25), the Rev-25 diagram and its superseded lowering note,
  §18.2e (superseded by §21.2), and §19 (resolved 2026-07-26), each
  under its original number; a stub with the same number stays in
  `compiler.md`, so every `§19` citation in tracking still resolves.
- The rule for the rest of the inline history: when a section is
  next revised, its dated notes older than that revision move to the
  history file under the same number (stated at the top of the
  history file). No inline note moved today: a note beside its rule
  carries the evidence for it, and moving 46 of them without a
  revision of their sections risks a wrong rule more than it saves
  reading.
- `README.md` "How it works" and the `codegen/src/lib.rs` crate doc
  said the dev tier lowers HIR to Cranelift; both now state the one
  LIR and its three consumers (§68).

## Measured

`compiler.md` 12,438 → 12,338 lines; the history file 351 lines.
`tools/hygiene.sh` exit 0; `cargo fmt --check` and
`cargo build -p subscript-codegen` 0 warnings for the doc comment
(`cargo doc` reports one pre-existing warning at `layout.rs:84`,
outside this change).
