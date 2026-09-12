<!-- §105 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 105. The `Array` namespace, and `Array.from`

Origin: `REPORT.md` item C. The name `Array` binds to no declaration,
so every member answers the general S016 `unknown name Array`. A
divergence attached to S016 fires on every unknown name and breaks
`r174-unknown-name`, so no member can carry a reject entry, and no
member carries a reason.

`js-api-sweep.md`'s standing rule (owner, 2026-07-25) reaches this
namespace: a JS API that exists and is implementable at realistic cost
is implemented, whatever the demand. Applied per member, the four
answers differ, which is why one namespace-wide statement was wrong.

Measured 2026-09-10 at `3d03f80` with TypeScript 5.9.2 and node
v24.18.0. Every form below is `tsc`-clean, the annotated forms
included: `Array.from(xs)`, `Array.from(set)`, `Array.from(fixed)`,
`Array.from("añ😀")`, `Array.from<i32>([])`, `Array.of<string>()`,
`Array.of<i32>(7)`, `Array.isArray(xs)`. On node,
`Array.from("añ😀")` has **3** elements, so a string yields one code
point per element, as §14.1 already does.

### 105.1 The namespace resolves

1. **`Array` binds to a builtin namespace through ordinary name
   resolution.** User shadowing follows the ordinary rules in **value**
   position. *(Recorded 2026-09-10, measured by the implementation
   round: in **type** position `Array<T>` is a builtin that
   `tyres.rs` maps to `T[]` before it consults declarations, so a user
   `class Array` does not shadow it. That predates §105 and this
   section does not change it.)*
2. **`r174-unknown-name` keeps its meaning.** No `Array`-specific
   divergence attaches to S016. Each member carries its own
   rejection and its own record.

### 105.2 `Array.from(source)` is accepted

1. **`Array.from(source)` is accepted** where `source` is `T[]`,
   `FixedArray<T, N>`, `Set<T>`, or a `string`. That is §14.4's
   operand list after §104. The result element type of a `string`
   source is `string`, one **code point** per element.
2. **The result is a fresh `T[]`, for every source, an array source
   included.** The source is evaluated once.
3. Element copy and ownership follow the rules array-literal spread
   already applies. This section adds none.
4. `stdlib.md` §14.3 owns the traversal. §103.1 built the sink; this
   member adds an array destination.
5. **An explicit type argument supplies the element type.**
   `Array.from<i32>([])` is accepted. `Array.from([])` with no type
   argument keeps §103.5's empty-array-literal S100, because the
   argument is checked with no contextual type.
6. **Rejected sources, each with its own reason**: a bare `Map`
   (§104.1); a `Generator<T>` (§14.4's single-use rule); a
   `keys()`/`values()` view (§14.1 accepts a view only as a direct
   `for…of` subject). **The view case is a restriction this section
   does not lift** — §104.2 records the path.
7. **The mapper overload `Array.from(source, fn)` is rejected**, and
   its reason is the callback typing and traversal work it needs, not
   "optional arguments". A round that implements it measures that
   work first.

### 105.3 The other three members

| member | this section | reason |
|---|---|---|
| `Array.isArray(x)` | **rejected, with its own record**; a candidate | The answer is statically known for an ordinary declared type, and a boundary-opaque `object` or a nullable form can need a runtime test. A round decides it after it inspects the runtime classification. The argument evaluates once whatever the answer, so a constant result is not a reason to drop it |
| `Array.of(a, b, …)` | **rejected, with its own record**; a candidate at fixed arity | Variable arity keeps the recorded variadic prerequisite. That prerequisite does **not** reach `Array.of<T>()` and `Array.of<T>(v)`: `push` and `unshift` already ship as fixed-arity forms. A round measures the dispatch and inference cost, then decides |
| `new Array<T>(n)` | **rejected** | The language has no array hole and no missing-element value. Filling with zero or `null` changes what a read means and what element presence means. This is not a cost statement |

**"A candidate" is not a rejection reason.** Each of the first two
rows names the measurement that decides it. Neither is refused.

**This table's wording is contract language, not diagnostic
language.** A row says "a round decides it after it inspects the
runtime classification", because a contract addresses this project. A
shipped message names the **measurement**, never this project's
schedule: a script author reading `generated-docs/api-reference.md`
has no rounds. *(Recorded 2026-09-10 after a round copied three of
these sentences into shipped messages.)*

### 105.4 Sites

- `compiler/src/ambient.rs`, the namespace and the member rejections.
- The checker's call path for the accepted member.
- The lowering, and the runtime sink's array destination.
- `specs/blocks/stdlib.md` §9, the `Array` surface.
- `specs/blocks/collisions.md` Q22.
- `specs/tracking/js-api-sweep.md`, the accepted table and the
  iteration table.
- `compiler/tests/corpus_reject.rs`, one line per new reject entry.

### 105.5 Corpus and gate (pre-registered exit criteria)

**Accept.** An `Array.from` battery over every §105.2 rule 1 source,
with: a `string` source whose text is not ASCII, so code-point
stepping is distinguishable from byte stepping; an array source, whose
result is shown to be independent of it by mutating one afterwards; a
`Set` source; a `FixedArray` source; an empty source; and
`Array.from<i32>([])`.

**Reject**, each at a pinned position with its rule code:
`Array.from(map)`; `Array.from(gen())`; `Array.from(m.keys())`;
`Array.from(xs, f)`; `Array.isArray(xs)`; `Array.of<i32>(1, 2)`;
`new Array<i32>(3)`.

**The round measures each entry's `tsc` class and writes it in the
header** (§103.8 rule 2). One is known to serve both classes:
`Array.from(map)` unannotated is `tsc`-accepted, and
`const a: i32[] = Array.from(m)` is `TS2322`, because `tsc` reads the
result as `[K, V][]`. **§79 rule 6 governs it** — the entry is the
unannotated form and the site carries a variant. The annotated form's
`tsc` code is pinned beside the corpus gate, by a test that **runs
`tsc` and compares**, per §104.6.

Each entry that is `tsc`-accepted renders the §79 block, and its
`collision` id names the record that **owns the reason**.

- **The three rejected sources reuse the variant their reason already
  owns**: a bare `Map`, a single-use `Generator`, and a held view.
  *(Corrected 2026-09-10, after the round reported the conflict. This
  paragraph said `compiler.md` §105.2 owned all three. §79 rule 1
  gives one variant per **topic**, and each of the three is a topic
  §104 or §14.4 or §14.1 already owns. A `§105.2` variant beside them
  would state `stdlib.md` §14.4's single-use rule in a second table
  row, which is the defect §103.8 rule 1 forbids. The view source
  cannot follow the letter at all: the existing `keys()` member check
  rejects `Array.from(map.keys())` before any `Array.from` rule sees
  a type.)*
- **The mapper overload and the three other members** take
  `compiler.md` §105.2 and §105.3, which own their reasons. §79
  rule 1 permits a `compiler.md` section id where `collisions.md` has
  no heading.

**A diagnostic names the rule that rejects, not the call the user
wrote.** `Array.from(map.keys())` reports on `keys`, because the view
rule fires first. That follows §103.8 rule 1, and this rule records it
so a reader of rule 6 does not expect otherwise.

**Gate.**

1. The differential gate is byte-exact on both tiers for every new
   accept entry, against a golden generated from the dev tier.
2. `tsc` reports zero errors over the accept corpus, configuration
   unchanged.
3. `node` runs each accept entry whose header says `js-comparable`,
   and agrees. **The round decides which entries are comparable and
   records why each other one is not** (§103.8 rule 2).
4. Each new accept entry is Red at this section's pin, and the round
   records the diagnostic it gave there.
5. The round reports which goldens and counted totals moved, and why.
   It does not predict them (§103.8 rule 2).
6. `tools/gate.sh full` green in both profiles.
