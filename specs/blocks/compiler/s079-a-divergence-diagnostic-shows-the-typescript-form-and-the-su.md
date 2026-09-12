<!-- §79 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 79. A divergence diagnostic shows the TypeScript form and the subscript form

*(Owner decision, 2026-08-30.)*

A reject diagnostic at a construct that `tsc` accepts is a divergence
from TypeScript. The diagnostic shows both forms in code.

**Rule 1 — one table.** `compiler/src/divergence.rs` defines
`Divergence`, one variant per divergence topic, and one table that
gives each variant: `ts` (a TypeScript fragment that `tsc` accepts),
`subscript` (the fragment this language accepts for the same intent,
or the sentence "no equivalent; <what to do instead>"), `why` (one
sentence, 25 words or fewer), and `collision` (the `collisions.md`
heading id, `C1`..`C14`, or a `compiler.md` section id when
`collisions.md` has no heading). No other place holds the fragments.

**Rule 2 — the diagnostic carries the fact.** `Diagnostic` gains
`divergence: Option<Divergence>`. A checker site that rejects a
construct `tsc` accepts passes the variant. A site that rejects a
construct `tsc` also rejects passes `None`.

**Rule 3 — the render.** After the `= rule:` line the renderer prints:

```
  = TypeScript accepts:
  |   <ts fragment, one line per line>
  = subscript:
  |   <subscript fragment>
  = why: <why> (collisions.md C7)
```

The block is absent when `divergence` is `None`. The existing lines
do not change, so every pinned message and position stays.

**Rule 4 — the gate is total.** `compiler/tests/corpus_reject.rs`
renders the expected diagnostic of every reject entry. An entry whose
header says `tsc: accepts` must render a divergence block. An entry
whose header says `tsc: rejects` must render none. The test reports
every violating entry at once (CLAUDE.md workflow: a total check,
not named sites).

**Rule 5a — a `subscript` fragment is never compiled. Open.**
*(Recorded 2026-09-11 by §107's round, which wrote a fragment that
does not compile and watched every §79 test pass on it.)* Rule 5's
tests check that a `collision` id names a heading, that every heading
has a variant, that the two fragments differ, that `why` is 25 words
or fewer, and that no fragment is empty. **None of the five compiles
anything.**

A `subscript` fragment is the advice a user acts on — "write this
instead" — so a fragment this compiler rejects is worse than no
fragment. The check is small and total: compile every `subscript`
fragment and require it to be accepted. A round that closes it
measures how many fragments fail **before** it proposes the check,
because that count is the work, and it decides then whether the `ts`
fragment gets the same treatment through the `tsc` harness.

**Rule 6a — an `ambient` rejection row is not reached by rule 4.**
*(Recorded 2026-09-10 by the §104/§105 Phase Review. Open; no round
is scheduled.)* Rule 4's total gate reads the **reject corpus**, so a
row in `compiler/src/ambient.rs` that no entry pins carries no
variant and nothing reports it. Measured `tsc`-accepted and rejected
here with no block: `const held = Array;`, `const f = Array.from;`,
`const p = Array.prototype;`, `const j = JSON;`, `const n = Number;`,
`const v = m.values();`, `const v = s.keys();`, and the `corpus:
None` rows for `isFinite(value)`, `new Number(value)`,
`toLocaleString`, `Date.parse`, `reduceRight(callback)`.

*(Second instance, 2026-09-11, from §107's review: a mirror `declare
function` with a pattern parameter is `tsc`-accepted and rejected here
with no block, because the `resolve_param_pat` arm outside the
boundary guard passes no variant.)*

This is a **form gap in rule 4**, not a defect of any one row, so a
named-site fix does not converge. The total check belongs where
`compiler/src/api_reference.rs` already walks every generated
rejection: assert each row's variant against its measured `tsc`
class. A round that closes it measures the class of every row first,
and reports how many need a variant.

**Rule 6 — a site can serve both `tsc` classes.** *(Added 2026-09-10
by §104's implementation round, which measured the conflict.)* Rule 2
is written per **site**, and a site can reject a program `tsc` accepts
and a program `tsc` rejects, when the acceptance depends on the
program and not on the construct. `[...m]` and
`const ks: i32[] = [...m]` reach one site.

The site's variant is therefore fixed for both. Two consequences
follow from rule 4, and a contract that pre-registers a reject entry
must respect them:

- **A site that carries a variant cannot host a `tsc: rejects` reject
  entry**, because the block always renders.
- **A site that carries none cannot host a `tsc: accepts` one.**

Choose the variant so the **user-facing** rejection explains itself,
which is the case the block exists for, and pin the other class with a
test that runs `tsc` and compares its code. **That test also pins the
site** — the `Divergence` variant the form reaches — because a form
that drifts to another site would keep passing a code-only check.
*(The site half was added 2026-09-11 by §107's round.)*

**Rule 5 — the table is checked against the record.** A unit test
reads `specs/blocks/collisions.md` (`include_str!`) and asserts that
every `collision` id in the table names an existing `### C<n>` heading,
and that every `### C<n>` heading has at least one variant. A second
test asserts every `ts` fragment differs from its `subscript` fragment
and every `why` is 25 words or fewer.

**Corpus.** No new entry: the 126 existing `tsc: accepts` reject
entries are the pins, and rule 4 makes each one carry a block.

### 79.1 A collision that rejects nothing carries no variant

*(Added 2026-09-08.)* §79's divergence block belongs to a rejection:
it shows the TypeScript form and the subscript form of a program the
checker refuses. A collision that names a behaviour difference and
rejects nothing has no diagnostic to carry it. Such an entry states
`No diagnostic reports this.` on its own line, and the totality check
that pairs every `collisions.md` heading with a `Divergence` variant
skips a heading that carries that line. A `Divergence` variant no
diagnostic can produce is a value that names no rejection, and the
enum stops being a total map of the rejections.
