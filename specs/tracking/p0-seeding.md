# P0 — seeding evidence

Status: in progress, 2026-07-22.

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

## Pending for P0 exit (plan §6)

- `tsc -p tsconfig.json` zero errors (requires `npm install`, run by the
  owner).
- Phase Review (CLAUDE.md): fresh no-context review; findings fixed in
  severity order.
