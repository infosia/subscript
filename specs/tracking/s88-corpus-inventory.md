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

## Round 2 (at `b1a5246`)

Red before §88 (complete copied entry `a999-inventory-red`): the
index byte-identity test, `corpus_accept`, `corpus_warn`,
`js_corpus` count pins, and the golden count fail. After §88 the
same copy fails only the index test. The 56 `INTERPRETER_EXCLUSIONS`
reasons moved unchanged into the entry headers; a22 carries
`// cost: benchmark`; `lir.rs` derives runnable (125) and debug (124)
from the headers; the five count pins and the three tables are
deleted; one header reader serves the generator and the suites.

**Golden change (§2 item 2).** Adding one header line to 57 accept
sources moves every source position in those entries by one line.
`codegen/tests/lir-goldens/corpus.txt` was regenerated through the
capture path (`SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`), never by hand. A
throwaway script paired every removed line with its added line:
4 entries (a95, a111, a128, a149), 1,127 replaced lines, 1,865
position numbers incremented by one, 0 lines with any other
difference, 0 unpaired lines. The reviewer reproduced the same
numbers independently. No `.expected` moved.

Fresh review: CRITICAL 0, MAJOR 2, MINOR 3. Both MAJOR were
contract defects the diff exposed: the trap table showed
`Interpreter = yes` from no fact any suite checks (the interpreter
runs the nine trap entries `DEBUG_INTERPRETER_TRAPS` names); rule 5
said the debug a22 omission printed a skip "as today", and it printed
nothing at HEAD, so exit criterion 4's count could not hold.
Corrected at `a3432cd`: the column on the accept table only; the
debug count is 2; `goldens-moved 1` named; the migration control is
deleted at landing. MINOR: a prefix match in the reader's
malformed-key check; a corpus.md sentence wider than rule 3; this
tracking entry. Round 3 takes the code items.

## Round 3 (at `a3432cd`)

The `Interpreter` column on the accept table only; the reader's
malformed-key check matches the first word; the migration control
ran once more (`interpreter_header_selection_matches_migration_control`,
1 passed) and is deleted. compiler 425, golden 35, LIR 37; clippy
7 / 18 / 13; workspace build 0 warnings.

The first full gate on this tree: `debug 1274/9/2 release 1272/9/2
skips 2/0 goldens-moved 1 exit 1`. The nine failures are
`cli/tests/gate.rs` cases that read the real `git status` and saw
this section's golden move (`s85-gate-command.md`, round 4); no §88
suite failed. `skips 2/0` and `goldens-moved 1` are the values §88.3
item 4 expects.
