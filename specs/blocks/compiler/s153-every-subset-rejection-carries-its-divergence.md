<!-- §153 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 153. Every subset rejection carries its divergence

*(Added 2026-10-03.)* Origin: §79 rule 6a, open since the §104/§105
Phase Review. The measurement is
`specs/tracking/s153-rejection-tsc-class.md`.

Problem: §79 rule 2 requires a variant at every site that rejects a
program `tsc` accepts. Rule 4 reads only the reject corpus, so a site
that no entry pins carries no variant, and nothing reports it.
`emit_api_rejection` selects the variant from the row's corpus file
name, so a row with `corpus: None` never carries one.

Measured at `5daba081` with one witness per row and per S014 site:
83 `rejected_api()` rows, 82 S014 construction sites, and the mirror
pattern-parameter site. 86 targets reject a `tsc`-accepted witness with
no block: 35 rows, 50 S014 sites, and the mirror site. Examples:
`const held = Array;`, `const j = JSON;`, `m.values()` held in a
`const`, `isFinite(x)`, `"x".match(/x/)`, `xs.flat()`. One `tsc` run
over all 318 witness files costs 0.27 s. The checker over all 315
programs in one process costs 0.05 s.

### 153.1 Rules

1. **Every S014 site carries a variant.** S014 rejects a standard
   library form that TypeScript has, so every S014 diagnostic is a
   divergence and renders a block. A site has no "no variant" case.
   §79 rule 6 permits the block on a `tsc`-rejected program at the
   same site.
2. **A row carries its variant.** `ApiRejection` gains
   `divergence: Divergence`. `emit_api_rejection` reads that field.
   The corpus-name selection is deleted.
3. **A row is read by the site that emits it.** A row that no checker
   lookup reads is deleted, or the direct site that emits its
   diagnostic reads the row. Measured: the three `REGEX_REJECTIONS`
   rows and the `JSON` `parse(text) without target type` row.
4. **An S014 site is named.** Every S014 diagnostic goes through one
   checker function that takes a site name from one closed enum. The
   enum maps each site to its `Divergence` in one exhaustive `match`.
   `RuleCode::S014` appears in no other checker file.
5. **Every row and every named site has a witness.** One table, keyed
   by the row and by the site enum through an exhaustive `match`,
   gives each target one or more witness programs that reach it, each
   labelled with its measured `tsc` class, or "unreachable" with the
   reason when no program reaches it.
6. **The check is total.** One test runs the checker on every witness
   and asserts the target's message and the target's variant in the
   block. The same test runs `tsc` once over all witnesses and asserts
   each label.
7. **A fragment is true.** Each variant that an S014 site or a row
   carries has a `ts` fragment that `tsc` accepts and that this
   checker rejects with a diagnostic carrying that same variant. Its
   `subscript` fragment is a program this checker accepts, or the
   §79 rule 1 sentence "no equivalent; <what to do instead>". One test
   checks every such variant; the set is derived from the site enum
   and the rows, not from a hand list. A site that only
   `tsc`-rejected programs reach carries a variant whose fragment
   meets this rule, so its block shows the nearest form that `tsc`
   accepts and this language rejects.
8. **Consequence for the corpus.** With §79 rule 4, no reject entry
   can pin a `tsc`-rejected S014 program. The rule 5 table pins such
   programs.
9. **Scope.** Rules 4 to 7 cover S014. The mirror pattern-parameter
   site (S100, `resolve_param_pat`) gets its variant as a named site.
   §153.3 records the other codes.

### 153.2 Acceptance

1. Red first: the rule 6 test, run against the pin's rows and sites,
   reports all 86 measured targets at once.
2. Each missing variant uses an existing `Divergence` variant when its
   `why` states the reason. Otherwise it uses a new variant, whose
   `collision` names the `stdlib.md` or `compiler.md` section of the
   Q-rule that the diagnostic already cites.
3. A reject entry whose header says `tsc: rejects` and whose site gains
   a variant breaks §79 rule 4. The corpus divergence gate reports
   every such entry at once; no list here is complete. Each entry
   changes its program to a `tsc`-accepted form that reaches the same
   site and keeps the entry's purpose, with the header `tsc: accepts`.
   Its old program becomes the site's rejected-class witness in the
   rule 5 table. If no `tsc`-accepted program has the site's
   diagnostic as its first diagnostic and keeps the purpose, the entry
   retires, and its old program stays as the witness. The tracking
   note lists each retirement with its measured first diagnostic.
4. An S014 message states no `tsc` outcome, because every S014 site
   serves both classes.
5. The rule 6 test states its measured cost in its doc comment.
6. No accept golden moves. A reject entry's pinned message does not
   change, except where acceptance 4 changes it.

### 153.3 Open

1. Codes other than S014 have no total check. A site of another code
   that rejects a `tsc`-accepted program carries a variant only if a
   reject entry pins it (§79 rule 4). No measurement of those codes
   exists.
2. A shared variant cites one `collision` section and shows one `ts`
   fragment, so a site of another Q-rule renders an example of another
   shape (`CompilerOwnedValue` at `const g = d.getTime;` shows
   `const held = Array;` and cites `stdlib.md` §9.0, while the
   diagnostic cites Q20). Recorded by the Phase Review; MINOR.
3. Rule 7 compiles the fragments of the variants that S014 sites and
   rows carry. The fragments of the other variants stay unchecked
   (§79 rule 5a).
