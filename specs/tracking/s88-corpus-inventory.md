# §88 — the corpus index is the inventory

Status: **in progress.** Contract: `specs/blocks/compiler.md` §88
(`74bc806`; corrected `b1a5246`), `specs/blocks/corpus.md` §1
(`e7b870f`). Origin:
`specs/tracking/development-cost-review-2026-09-05.md` finding 4.

## Round 1 (at `f2c2481`) — stopped at the Red fixture

The contract's Red ("one accept entry copied to a new id and no
other edit") could not reach the byte comparison: the header check
of `render_corpus_index` rejects a copied `// corpus:` line before
it, and `js_corpus.rs` requires the `.expected` beside every
top-level accept source. Measured with the plain copy: the
language-reference test failed at the header check; `corpus_accept`,
`corpus_warn`, and `js_corpus` failed on their count pins (181 vs
180, 183 vs 182, 181 vs 180); `golden` passed (its count reads
`.expected` files). Contract corrected: the fixture is a complete
copied entry (header id and `.expected`). Found by the coding
agent's Red run, as the handoff required.
