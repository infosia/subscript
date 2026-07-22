# P0 — seeding evidence

Status: COMPLETE, 2026-07-22.

## Landed

- Founding record: `CLAUDE.md`, `specs/subscript-project-plan.md` Rev 0.
- Blocks: `specs/blocks/corpus.md`, `specs/blocks/collisions.md`,
  `specs/blocks/compiler.md` — all Rev 0.
- Corpus: 24 accept entries (`a01`–`a24`, `a19-modules/` two files),
  14 reject entries (`r01`–`r14`).
- Prelude: `prelude/lang.d.ts`. `tsc` harness: `tsconfig.json`,
  `package.json` (devDependency `typescript@5.9.2`).
- Reference sweep over all tracked files (predecessor-project names,
  legacy host-API names, paths outside the repository): zero matches.

## Phase Review (2026-07-22)

Fresh no-context review: 0 CRITICAL, 3 MAJOR, 6 MINOR. All addressed:

- MAJOR: `a12` instantiated a `@value class` with a `string` field,
  contradicting C2's field whitelist → second instantiation changed to
  `Box<f64>`.
- MAJOR: plan provenance cited pre-founding evidence the repository
  cannot support → rewritten to claim only owner-approved decisions;
  model validation re-anchored to the P3/P4 gates.
- MAJOR: Perry and Mystral Native citations flagged as unverifiable →
  verified live (2026-07-22 fetch): both repositories exist and match
  the table's descriptions. No change; *(docs)* markers added (MINOR).
- MINOR: Q12's a23 description aligned with the file (its own `main`
  drives the trio); Q14/Q15/Q17 added to under-reporting headers
  (`a04`, `a12`, `a22`–`a24`); JS truncation idiom `>>> 0` removed from
  `a22`; CLAUDE.md principle 3 scoped to "from the second execution form
  (P3)"; two rhetorical phrases removed.

Review verdict: cross-reference web (Q-ids, C-rules, phases, entry
lists, prelude declarations) fully resolves; forbidden-reference sweep
clean.

## Exit (plan §6)

- `tsc -p tsconfig.json` (typescript 5.9.2): exit 0, zero errors, over
  `prelude/lang.d.ts` + all 25 accept files (2026-07-22).
- Reference sweep: zero matches. Phase Review: no open findings.
- Next: P0.5 — mobile link spike (plan Rev 1), then P1.
