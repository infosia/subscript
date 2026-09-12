<!-- §108 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 108. A field carries a value before a constructor returns

Origin: §106's Phase Review, which measured a program the checker
accepts and stock `tsc` rejects. Invariant 5 says every accepted
program type-checks under stock `tsc`. This one does not, and it also
reads a reference that is null in a language whose only null is
`Ref | null`.

Measured 2026-09-10 at `a7c4977`, TypeScript 5.9.2 with the repository
`tsconfig.json` and `prelude/lang.d.ts`:

| program | this language | stock `tsc` |
|---|---|---|
| `class H { x: i32; }` then `h.x` | accepted; prints `0` | `TS2564` |
| `class H { inner: Inner; }` then `h.inner.v` | accepted; **dev tier dies with signal 11** | `TS2564` |
| `class H { g: Generator<i32>; }` then `h.g.next()` | accepted; dev tier exits 2 | `TS2564` |

`strictPropertyInitialization` is part of `strict`, which
`tsconfig.json` sets, so `tsc` requires a field to have an
initializer, to be definitely assigned in the constructor, to be
optional, or to carry a `!` assertion.

**Two defects, one cause.** The program fails the `tsc` gate that
invariant 5 requires, and the second row dereferences a null through a
field whose type is not nullable. A segmentation fault is not a trap:
the language's failure channel is a trap with a kind, and C5's memory
model has no null reference outside `Ref | null`.

**Nothing in the corpus breaks.** The accept corpus is `tsc`-clean by
gate, so no entry can hold this shape. The fix retires no working
program, and that is why it is cheap.

### 108.1 The rule

1. **A declared field has an initializer, or the constructor assigns
   it.** Otherwise the class is rejected, at the field. **A `!`
   assertion does not satisfy the rule.** *(Added 2026-09-11,
   measured: `class H { x!: i32; }` is accepted today and `tsc`
   accepts it, so invariant 5 does not force this — C5's memory model
   does. The assertion tells `tsc` to trust the author; this language
   has no runtime null check on a non-nullable reference, so a
   reference field left unassigned behind `!` is the signal-11 row of
   the table above with a different spelling.)*
2. **The assignment must be unconditional at the constructor's top
   level, and no statement before it can leave the constructor.** A
   top-level `this.f = …` counts only if every statement before it
   contains no `return` from the constructor, at any statement depth.
   A lambda body is a separate function; its `return` does not count.
   *(Corrected 2026-09-12: the
   first implementation read "top level" as "is a top-level
   statement", so `if (flag) { return; } this.inner = new Inner();`
   was accepted — `tsc` answers `TS2564`, and the run dies with
   signal 11.)* `tsc` accepts a definite assignment through both arms
   of a conditional; this rule does not. **Stricter than `tsc` is
   permitted** — invariant 5 asks that everything this language
   accepts, `tsc` accepts, not the reverse — and a definite-assignment
   analysis is a larger change that this section does not need.
3. **Three field kinds are outside rule 1, each for a measured
   reason.** *(Rewritten 2026-09-11; the first draft said "an optional
   field and a `!` assertion stay outside", which a round can read as
   "reject" or as "accept".)*
   - **An ambient declaration.** A `declare class` — in a `.d.ts`
     mirror, or in a program file — has no initializer and no
     constructor: the host, or the declaration's own author, populates
     it. `tsc` exempts an ambient declaration from `TS2564`, and it
     answers **`TS1039`** for an initializer inside one, so **neither
     spelling rule 1 offers is legal there**. A program-file ambient
     class has no producer in a program file: §108.4 rule 5 rejects
     `new`, and a program-file `declare function` is S100 "function
     bodies are required". It names a type only. *(Added 2026-09-12
     by the Phase Review.)* Rule 1 does not reach an
     ambient declaration, **generic or not** — a generic template's
     instances inherit its ambient status. *(Corrected 2026-09-12: the
     first draft named "a declaration checked under the boundary
     flag", which covers the 89 `corpus/interop` mirrors — all 89
     carry such a field, 259 fields in all — and misses a program-file
     `declare class`. The first implementation then reached
     `declare class Ext<T> { value: T; }` through its instances, which
     the pin accepted and `tsc` accepts.)*
   - **An R16 absence-capable member** — `name?: A` inside a
     `@Descriptor` class — is accepted by R16 and by `tsc`. Outside a
     `@Descriptor` class an optional field is already S012 ("optional
     properties imply `undefined`"), so rule 1 never sees one.
   - **An R17 required Descriptor member** — `name!: T` inside a
     `@Descriptor` class — is the spelling R17 contracts for a member
     the literal must supply. `tsc` accepts it, and `a92`, `a117` and
     `a149` hold it. Rule 1's `!` clause reaches an ordinary class
     only. *(Added 2026-09-12 after the implementation round measured
     the three entries.)*
   - **A static field** without an initializer is already S100
     ("static fields require an initializer"), which is stricter than
     `tsc` and predates this section. Rule 1 leaves it to that rule.
4. The diagnostic names the field and names the two spellings that
   satisfy the rule. **It says that `tsc` answers `TS2564` only where
   `tsc` does** — a bare field with no assignment, or a field whose
   top-level assignment follows a statement that can return from the
   constructor. For the `!` form and for a field
   assigned in both arms of a conditional, `tsc` accepts the program,
   the diagnostic renders the §79 block instead, and the block's
   `why` carries the reason. *(Corrected 2026-09-12: the first draft
   claimed `TS2564` for every form.)*

### 108.1a Found by the round, decided in §108.4

*(Recorded 2026-09-12 from the implementation round's report and
from the Phase Review. Decided 2026-09-12 by §108.4.)* Two spellings
of this section's table row stayed open after rules 1–4:

- A program-file `declare class Ext { inner: Inner; }`, constructed
  with `new Ext()` and read as `e.inner.v`. Rule 3 exempts the
  declaration, so the field stays null and `new` hands it out.
- A read of a field before the top-level statement that assigns it:
  a `print` of the field, `this.inner = this.inner`, a method call
  that reads the field, or a parameter default that reads it.

Each form checks clean at `4148eab` and dies with signal 11 on both
tiers. §108.4 measures every form and takes the two decisions.

### 108.2 Sites

- The checker's class declaration pass.
- `check_new` (`compiler/src/check/expr.rs`), rule 5, beside the
  opaque-handle rejection. *(Added 2026-09-12.)*
- The constructor pass (`check_class_body`,
  `compiler/src/check/mod.rs`), rule 6, over the HIR constructor
  body and the parameter defaults. *(Added 2026-09-12.)*
- `compiler/src/divergence.rs`, two variants for §108.4's two
  `tsc`-accepted sites. *(Added 2026-09-12.)*
- `specs/blocks/collisions.md` C9, which contracts field initializers
  and says nothing about a field without one. *(Pointer added
  2026-09-12.)*
- `specs/blocks/compiler.md` §57, R27's field-initializer section.
  *(Pointer added 2026-09-12.)*

### 108.3 Corpus and gate (pre-registered exit criteria)

**Reject**, each at a pinned position with its rule code: a scalar
field with no initializer; a reference field with no initializer; a
`!` field with no initializer and no assignment; and a field assigned
in **both** branches of a constructor conditional. **Four entries.**
*(Corrected 2026-09-12. The first draft asked for a fifth, the
**one**-branch form. It shares a checker site with the both-branch
form; §79 rule 6 fixes that site's variant, and a `tsc: rejects`
entry cannot sit at a site that carries one. The one-branch form is
pinned by the `RecordedForm` test that runs `tsc`, as rule 6
prescribes.)*

**The round measures each entry's `tsc` class** (§103.8 rule 2).
Measured 2026-09-11 for two forms, so the shape is known: the
one-branch form is `TS2564`, and it lives in the `RecordedForm` test
rather than the corpus; the both-branch form is **`tsc`-accepted** — `tsc` follows definite assignment through both
arms and rule 2 does not — so that entry renders the §79 block, and
its `collision` id names this section. That entry is the one place
rule 2 retires a program `tsc` accepts, and it is safe: both arms
assign. The record says so, and names a definite-assignment analysis
as the path back. *(The first draft said every reject entry is
`tsc: rejects`. It predicted a measurement, and the prediction was
wrong for the both-branch form.)*

**Accept**: a class whose fields are all initialized; a class whose
constructor assigns every field at its top level; and both together.

**Gate.**

1. Each new reject entry is Red at this section's pin — that is, the
   pin **accepts** it — and the round records what it printed there.
2. The round reports every corpus entry, example, benchmark, test,
   and **interop mirror** that stops compiling. Rule 2 can reach a
   constructor that assigns inside a conditional, and rule 3's mirror
   exemption is what keeps the 89 `corpus/interop` classes compiling;
   both are the round's measurement, not this section's claim.
3. `tsc` reports zero errors over the accept corpus, configuration
   unchanged.
4. The round reports which goldens and counted totals moved, and why.
5. `tools/gate.sh full` green in both profiles.

### 108.4 Rules 5 and 6 — `new` on an ambient class, and `this` before the assignment

*(Added 2026-09-12. Closes §108.1a.)*

**Measured at `4148eab`**, with the repository `tsc`, its
`compilerOptions`, and `prelude/lang.d.ts`; `check` and the dev-JIT
`run`. `Inner` has `value: i32 = 3`, and `Holder` has `inner: Inner`
with `this.inner = new Inner();` at the constructor's top level.

| program | `tsc` | `check` | `run` |
|---|---|---|---|
| program-file `declare class Ext { inner: Inner; }`, `new Ext()`, read `e.inner.value` | clean | clean | signal 11 |
| program-file `declare class P { x: i32; constructor(x: i32); }`, `new P(5)`, print `p.x` | clean | clean | prints `0` |
| `print(...)` of `this.inner.value` before the assignment | TS2565 | clean | signal 11 |
| `this.inner = this.inner` as the assignment | TS2565 | clean | signal 11 |
| the `print` inside `if (flag) { … }` before the assignment | TS2565 | clean | signal 11 |
| `constructor(n: i32 = this.inner.value)` | TS2565 | clean | signal 11 |
| `this.show()` before the assignment; `show` reads the field | clean | clean | signal 11 |
| `show(this)` before the assignment; `show` reads the field | clean | clean | signal 11 |
| a lambda that reads `this.inner`, stored before the assignment | not run | S100 "`this` is only available in constructors and methods" | not run |
| `this.count = this.count + 1` (initialized), `this.first = new Inner()`, `this.inner = this.first`, then `this.show()` | clean | clean | `3 2` then `3` |

**Rule 5 — `new` on an ambient class that is not a mirror is
rejected, at the `new`.** A mirror class, one a `.d.ts` file
declares, is outside the rule. `lower_new` (`codegen/src/lir.rs`)
stores every argument into the field at the same position. That is
the mirror constructor's contract. A program-file `declare class`
has no constructor body and no positional store, so no argument
reaches a field; the table's second row prints `0` for `new P(5)`.
The host populates a program-file ambient class through a `declare
function`, or nothing does. The diagnostic follows the opaque-handle
one: the class is obtained from the host, not constructed, because a
`declare class` has no constructor body. `tsc` accepts every form,
so the site carries a variant, `collision: "compiler.md §108"`. A
generic `new Ext<i32>()` on `declare class Ext<T>` reaches the same
site through its instance.

**Rule 6 — `this` before the assignment prefix ends.** *(Rewritten
2026-09-12 by the Phase Review; the first form was positional.)*

Definitions:

- A *rule-1 field* is a field with no initializer that rule 1
  reaches.
- A field *holds a value* at a statement when it has an initializer,
  or when a top-level statement earlier in the prefix assigns it.
  That is rule 2's notion. A nested assignment does not count. The
  statement's own target does not count.
- The *assignment prefix* of a constructor starts with its parameter
  defaults. It continues with the top-level statements, up to and
  including the first one after which every rule-1 field holds a
  value. If no such statement exists, the prefix is the whole
  constructor. Rule 1 rejects that class as well, and both report.
- A class with no rule-1 field has an empty prefix.

Inside the prefix, at any statement or expression depth, `this`
appears in two forms only:

- (a) the target of an assignment `this.f = …`, top-level or nested;
- (b) a read `this.g` where `g` holds a value at that statement. A
  read is every use that is not form (a): an operand, a member write
  `this.g.x = …`, a compound assignment `this.g += …`, an increment,
  an argument, and the receiver `this.g.m()`.

Every other appearance is rejected at the `this`:

- **Site A**: a read `this.g` where `g` does not hold a value. The
  message names `g`. `tsc` answers TS2565 for a read that no
  assignment precedes, so the site carries no variant. One class of
  `tsc`-accepted programs reaches it: a read after a nested
  assignment of the field and before its top-level assignment. `tsc`
  follows the nested assignment; rule 2 does not. A test that runs
  `tsc` pins one form per statement shape and asserts the site (§79
  rule 6). The shapes: both arms of a conditional, the same arm, a
  loop body, and a chained assignment `this.a = this.b = …`.
- **Site B**: a method or accessor call on `this`, or `this` as a
  value — an argument, an initializer, an assignment source, a return
  value. The message names every rule-1 field that holds no value
  there. `tsc` accepts these (the table's rows 7 and 8): its
  definite-assignment analysis does not follow a call. The site
  carries a variant, `collision: "compiler.md §108"`, and its `why`
  states that the callee can read a field that holds no value.

A parameter default evaluates after the field initializers (§57.1
step 4), so an initialized field holds a value inside a default. A
lambda body cannot mention `this` (§57.1, C9), so rule 6 does not
reach one. After the prefix every rule-1 field holds a value, and
the rule ends. Rule 1 and rule 6 are independent: a class that
violates both reports both.

Each rule 6 diagnostic names two spellings: move the use after the
assignment of the named field or fields, or give each named field an
initializer. Site A names the read field. Site B names every rule-1
field that holds no value at the use, and the prefix definition
makes that list non-empty.

**Corpus.** Reject, four entries, each S100 at a pinned position:

1. `new` on a program-file `declare class` (row 1): `tsc: accepts`,
   block, at the `new`.
2. A `print` of the field before its assignment (row 3):
   `tsc: rejects TS2565`, no block, at the `this`.
3. A method call before the assignment (row 7): `tsc: accepts`,
   block, at the `this`.
4. `this` as an argument before the assignment (row 8):
   `tsc: accepts`, block, at the `this`, same site as entry 3.

Test-pinned forms, in the `RecordedForm` shape (§79 rule 6): each
runs `tsc`, compares its code, and asserts the `Divergence` variant
or its absence.

- `new P(5)` with a declared bodiless constructor: `tsc` accepts;
  rule 5 site.
- `new Ext<i32>()` on `declare class Ext<T>`: `tsc` accepts; rule 5
  site.
- `this.inner = this.inner`: TS2565; site A.
- The nested read: TS2565; site A.
- The parameter-default read of a rule-1 field: TS2565; site A.
- The four nested-assignment forms above: `tsc` accepts; site A, no
  block.

Accept, two entries, `js-comparable: yes`:

- Row 10's shape: an initialized field read and reassigned, an
  earlier-assigned field read, then a method call and `this` as an
  argument after the prefix.
- A parameter default that reads an initialized field, `count: i32 =
  5` and `constructor(n: i32 = this.count)`, with the golden `5 5`
  (§57.1 step 4). *(Added 2026-09-12 by the Phase Review.)*

**Gate.**

1. Each reject entry is Red at this section's pin: the pin accepts
   it, and the round records what `check` and `run` printed there.
   The table above is the measurement at `4148eab`.
2. The round reports every corpus entry, example, benchmark, doc
   block, test, and interop mirror that stops compiling. A regex
   pass (2026-09-12) read 140 constructors with a rule-1 field in
   `corpus/accept`, `corpus/warn`, `corpus/trap`, `examples`, and
   `benchmarks`. It found no `this` inside a prefix beyond forms (a)
   and (b). The round's build is the measurement.
3. `tsc` reports zero errors over the accept corpus, configuration
   unchanged.
4. The round reports which goldens and counted totals moved, and
   why. The reject count and the divergence table size move.
5. `tools/gate.sh full` green in both profiles.

**After the implementation round.** *(Recorded 2026-09-12 from
`REPORT-113`.)* The site A message names no `tsc` code: a
`tsc`-accepted class reaches the site, and §108.1 rule 4 forbids the
claim where `tsc` does not answer it. Blast radius, measured: one
test built `new Box()` on a program-file `declare class Box` for a
receiver, and now takes the receiver as a parameter. No corpus
entry, example, benchmark, doc block, or interop mirror stopped
compiling.

**Phase Review, 2026-09-12.** One CRITICAL, two MAJOR, seven MINOR.

- CRITICAL: form (b) said an initialized field holds a value inside
  a parameter default, and both tiers evaluated the default before
  the initializers. Measured: `count: i32 = 5`, `constructor(n: i32
  = this.count)` printed `0 5` on both tiers; `node` prints `5 5`. A
  reference field in the same position died with signal 11. Fixed
  in §57.1 step 4: the tiers move the default evaluation after the
  initializers. Both tiers agreed, so no golden saw it (core
  principle 12); the new accept entry is the record.
- MAJOR: site A was reached by four `tsc`-accepted forms and the
  text named one. Fixed above: the class is named, and one form per
  statement shape is pinned.
- MAJOR: the prefix ended at the last rule-1 assignment. A site B
  use after every field held a value was rejected, the message named
  no field, and the advice changed the program. Fixed above: the
  prefix ends at the first statement after which every rule-1 field
  holds a value.
- MINOR, fixed, six: the after-round note said rule 1 reports alone
  when no top-level statement assigns the field. A parameter default
  that reads the field also reports site A, and `tsc` answers TS2564
  and TS2565 there. Eight sentences were over 25 words. Rule 3 named
  a producer that a program-file ambient class does not have. No test
  pinned the receiver form `this.g.m()` at site A. The text did not
  say which site names which fields. The text did not say how many
  spellings the diagnostic names.
- MINOR, open: a generic class reports rule 6 once per instance at
  the template position, as rule 1 does. A class-level rule reports
  once. Recorded, not fixed here.
