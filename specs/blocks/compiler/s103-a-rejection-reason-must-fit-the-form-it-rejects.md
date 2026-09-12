<!-- §103 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 103. A rejection reason must fit the form it rejects

*(Owner decisions 2026-09-10.)* Origin: a documentation round
measured every regex and iteration form against the compiler, because
`js-api-sweep.md` still listed a regex engine and an iterator protocol
as missing prerequisites six weeks after P22 and P23 shipped both.
The round found five recorded reasons that no longer fit the form
they reject.

Measured at `be15ee3` on aarch64 macOS, both tiers, against
TypeScript 5.9.2 and node v24.18.0:

| recorded reason | the form it rejects | what the measurement gives |
|---|---|---|
| "`new Map([[k, v]])` requires a pair element" | `new Set<i32>([1, 2, 3])` | the form has no pair element |
| "subject-only fused view … receiver is `Bag`" | a user method named `values()` | the same class runs when the method is named `each()` |
| "`match` **fails stock `tsc`** … Invariant 5 excludes it" | `"s".match(re)` | the call type-checks; only the `index` read is TS2322 |
| "`match`/`matchAll`/`search` (a regex engine)" | `s.search(re)` | `search` shipped in P23 |
| "mutable state on a **value**" | `re.lastIndex` | §15.5a makes a `RegExp` an ordinary Context allocation |

Being recorded makes a rejection decided. It does not make its reason
true. Core principle 14 states the rule; this section applies it.

**Owner decisions, 2026-09-10.**

1. `new Set(source)` is implemented (103.1).
2. The result type that `exec`, `match`, and `matchAll` need is
   **deferred. It is not refused**, and 103.5 keeps it open.
3. A view check reads the receiver type (103.2).
4. Four normative reasons retire (103.3).
5. A `RegExp` is a handle (103.4).
6. The corpus gains the entries 103.7 names.

### 103.1 `new Set(source)` is accepted

1. **`new Set<T>(source)` is accepted** where `source` is `T[]`,
   `FixedArray<T, N>`, `Set<T>`, or — for `Set<string>` — a `string`.
   The result is a fresh `Set<T>`.
2. `T` must be a Q24 key kind (`stdlib.md` §10.2), as `add` requires.
3. **First-occurrence order.** Duplicates collapse under
   SameValueZero and `-0` normalizes to `+0` at insert. Both rules
   are Q24's. This form adds none.
4. **The lowering is one instruction over the traversal that
   array-literal spread already uses.** No iterator object is
   created, on any tier. A `string` source yields one code point per
   element, as §14.1 does.

   *(Amended 2026-09-10, after the implementation round reported the
   collision.)* This rule first read "§14.3's fused traversal with
   `add`". Under the obvious reading that lowers to `Set.New` plus
   one synthesized `Set.add` per element, and **§83.1 rule 4 forbids
   a second synthesized call** that no HIR node spells: "This is the
   only entry a HIR walk cannot see; a second such entry needs a rule
   here first." One instruction, with the traversal in the runtime
   behind a shared sink, needs no such entry and no rule change.
   Duplicate collapse still comes from the one insert path that
   `add` calls, so rule 3 holds by construction rather than by a
   second implementation.
5. **A `Map` source is rejected, and invariant 5 is the reason.**
   Stock `tsc` answers TS2769 for `new Set<i32>(map)`, because
   `Map<i32, string>` is `Iterable<[i32, string]>` and not
   `Iterable<i32>`. Measured with TypeScript 5.9.2. The diagnostic
   names invariant 5 and the `tsc` code.
6. **A `Generator<T>` source is rejected**, by §14.4's rule: a
   generator is single-use, and construction is a value expression.
7. **`new Map(source)` stays rejected in every form.** A pair element
   needs a tuple type.

The reason for the change is the reason that was there. One rejection
row in `compiler/src/ambient.rs` covered both constructors and gave
the tuple gap. That gap is real for `Map` and absent for `Set`. With
the false reason gone, `js-api-sweep.md`'s standing rule reaches the
form: a JS API that exists and is implementable at realistic cost is
implemented, whatever the demand.

### 103.2 A view check reads the receiver type

1. The subject-only restriction on `keys()` and `values()`, and the
   rejection of `entries()`, apply **only** when the receiver is
   `T[]`, `FixedArray<T, N>`, `Map<K, V>`, or `Set<K>`.
2. On any other receiver the name is an ordinary member, and the
   ordinary member rules decide it.
3. This **removes** a restriction. No program that checked clean
   before stops checking clean.
4. **The subject environment does not depend on the member name.**
   A `for…of` subject is checked under the same flags whatever its
   member is called. *(Added 2026-09-10 by the Phase Review, which
   measured the first implementation of rule 2 failing it: with a
   method named `each`, `for (const v of b.each<object>())` checked
   clean; with the same method named `values`, it answered S011,
   because the receiver-typed path returned before the subject flag
   was set.)* A unit test pins one program under both spellings.

Measured at `be15ee3`, before this section: `class Bag { each():
Generator<i32> }` with `for (const v of bag.each())` ran on both
tiers and was `tsc`-clean, while the same class with the method named
`values()` was rejected with "subject-only fused view … receiver is
`Bag`", and named `entries()` with the tuple gap, which that program
does not contain. `stdlib.md`
§14.1 restricts the built-in receivers and says nothing about a user
class, so the check exceeded the contract it cites.

### 103.3 Four reasons retire from the normative text

Each line below is normative today and states a reason the
measurement retired. A contract outranks a tracking file, so a reader
who consults the contract gets the old reason until these move.

| site | replacement |
|---|---|
| `stdlib.md` §8, the rejected-`String` paragraph | `search` leaves the missing-prerequisite list. `match` and `matchAll` stay, and cite the missing result type, not the engine |
| `stdlib.md` §14.3, the subject-position paragraph | the obstacle is a missing language type for a held view, plus §70.1 decision 1's scope for the reference-counted handle. Drop "the first escaping temporary in the language, and a memory-model change": `Generator<T>` already escapes the call that made it |
| `stdlib.md` §15.3, the `match` bullet, and `collisions.md` Q31 | the call type-checks under stock `tsc`. Only `const i: i32 = m.index` is TS2322, because `RegExpMatchArray.index` is optional. The obstacle is the missing result type; invariant 5 narrows to the `index` read |
| `stdlib.md` §15.3, the `exec` bullet | the missing shape is an array with named extra fields. Drop "and no tuple type": `RegExpExecArray` extends `Array<string>` with a required `index` and `input`, and its captures are variable-length |

### 103.4 A `RegExp` is a handle, not a value

`stdlib.md` §15.3 calls `lastIndex` "mutable state on a **value**".
§15.5a calls a `RegExp` handle "an ordinary Context allocation", and
`matchStart`/`matchEnd` already expose that handle's match state. The
two descriptions do not agree.

A `RegExp` is a handle. §15.5a is the description the contract keeps.
`lastIndex` stays rejected, and its reason is that it exists to drive
`exec`, whose result type the language does not have. The sticky flag
`y` steers matching through `lastIndex` and stays rejected with it.

### 103.5 What stays open

None of these is refused. Each one waits for a decision, and a later
reader must not read this list as a rejection.

1. **The array-with-named-fields result type** that `exec`, `match`,
   and `matchAll` need. Deferred by the owner on 2026-09-10. The
   three forms share one missing shape, so one decision governs all
   three.
2. **`matchAll`'s fusion decision** under Q30/§14.3. §14.3 fuses an
   index loop over a container's own storage, and `matchAll` has
   none, so §14.3 decides nothing for it.
3. **`new Map(otherMap)`.** It needs no user-visible tuple and is the
   same class as 103.1's `Set` form. 103.1 keeps it rejected because
   the owner approved the `Set` form only.
4. **`a217` did not run on the reference interpreter.** *(Closed
   2026-09-10 by §106, which found the defect wider than this item
   states and repaired every packed location. `a217` now has three
   witnesses and carries no exclusion.)* Both
   production tiers ran it and agreed with the golden. The
   interpreter answered "expected runtime handle, found Coroutine"
   when the program stores a `Generator<T>` in a class field: it
   holds a generator as a coroutine value and has no
   `Type::Generator(_)` pack and unpack pair, where an async handle
   has one. Only **storage** packs, so a generator that a program
   passes or returns is unaffected. `a217` therefore carries the
   exclusion and has two witnesses, not the three core principle 12
   asks for. **`a216` holds the pass and return forms, keeps all
   three witnesses, and is what retires §103.3's
   escaping-temporary reason** — the split exists so that retirement
   rests on three witnesses. The fix is a generator registry plus the
   matching arms, and it changes the interpreter's value model.
5. **`for (const [k, v] of map)` has no record.** The rejection is
   real and S100 names it, and no section under `specs/blocks/`
   decides destructuring, so §79 rule 4 has no id to cite. A reject
   entry needs that record first.
6. **`Array.from(xs)` reaches only the general S016.** A named
   rejection for the `Array` constructor namespace is a checker
   feature, not a corpus entry. A divergence on S016 itself fires on
   every unknown name.
7. **`new Set<i32>([])` is rejected**, with S100 "cannot infer the
   type of an empty array literal without context". The source
   operand is checked with no contextual type, because §103.1 rule 1
   accepts four type shapes. No rule requires the contextual form.
   Measured 2026-09-10; recorded, not decided.
8. **`[...map]` disagreed with stock `tsc` about the element type.**
   *(Closed 2026-09-10 by §104. The measurement that opened this item
   named the spread; §104 found the same hole in `for…of` and rejects
   both forms.)*
   Measured 2026-09-10: `const ks: i32[] = [...m]` checks clean here
   and prints the keys on both tiers, while `tsc` answers TS2322,
   because it reads the spread as `[i32, string][]`. Invariant 5 says
   an accepted program type-checks under stock `tsc`, and this one
   does not. `a81` is `tsc`-clean only because its `[...map]` binding
   carries no type annotation, so the two systems infer different
   element types and neither reports. **No decision is taken here**;
   the fix changes what `[...map]` means, or rejects it.

### 103.6 Sites

- `compiler/src/ambient.rs`: split the `new Map/Set(iterable)`
  rejection row. `Set` gains an accepted form, a `Map`-source
  rejection naming invariant 5 and TS2769, and a `Generator`-source
  rejection naming §14.4.
- The checker's view check: gate the `keys`/`values`/`entries`
  rules on the receiver type (103.2).
- `specs/blocks/stdlib.md`: §8, §10.4, §14.1, §14.3, §15.3.
- `specs/blocks/collisions.md`: Q30, Q31.
- `compiler/tests/corpus_reject.rs`: one `EXPECTED` line per new
  reject entry, with its rule code and line.
- `codegen/tests/lir-goldens/corpus.txt`: it collects every accept
  entry that declares a generator or an async function, so every
  generator entry 103.7 requires grows it. Regenerate it with
  `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`.

### 103.7 Corpus and gate (pre-registered exit criteria)

**Accept.**

- A `new Set(source)` battery over `T[]`, `FixedArray<T, N>`,
  `Set<T>`, and `string`, with a duplicate that collapses, a `NaN`
  element, a `-0` element, and the resulting order observed.
- A user class whose methods are named `keys`, `values`, and
  `entries`, each returning `Generator<T>`: each driven by `for…of`,
  and one driven by hand with `.next()`.
- A `Generator<T>` passed to a function and returned from a
  function, driven by `.next()` and by `for…of`. This entry carries
  103.3's retirement of the escaping-temporary reason, and no entry
  pins those forms today.
- A `Generator<T>` stored in a class field, in its **own** entry.
  *(Split from the entry above on 2026-09-10.)* The reference
  interpreter could not run the storage form, so this entry carried
  an exclusion and the entry above kept three witnesses. **§106
  repaired the interpreter the same day**, and the exclusion is gone;
  the split stays, because the two entries pin different forms.

**Reject**, each at a pinned position with its rule code:
`new Set(map)` naming invariant 5 and TS2769; `new Set(gen())`
naming the single-use rule; `new Map(otherMap)` naming the tuple
gap; `new C(...xs)` naming variadic parameters; `[...gen()]`;
`{...a}`.

**The round records, per entry, the diagnostic at the contract pin,
and every diagnostic that moved anywhere else.** Core principle 10
asks whether an entry was Red. That answer is a measurement, and this
section does not predict it. `[...gen()]` and `{...a}` pin forms the
language already rejects; whether landing them moves a diagnostic for
programs outside the corpus belongs to the record the round reports.
The gap those two close is core principle 2's — a decided form with
no entry.

*(Amended 2026-09-10. This list named two more entries,
`for (const [k, v] of map)` and `Array.from(xs)`, and they cannot be
written. §79 rule 4 is a standing total gate: a reject entry whose
header says `tsc: accepts` must render a divergence block, and §79
rule 5 requires that block's id to name a record. Both forms are
`tsc`-accepted and neither has a record. Measured: the first
diagnostic for the destructuring form is S100 "destructuring is not
in the decided surface", and the word `destructuring` appears in no
file under `specs/blocks/`; the first diagnostic for `Array.from` is
the general S016 "unknown name `Array`", and a divergence there
fires on every unknown name and breaks `r174-unknown-name`, whose
header says `tsc: rejects TS2304`. Writing exit criteria that §79
forbids was a defect of this section. §103.5 items 5 and 6 hold the
two forms as open.)*

**Gate.**

1. The standing differential gate is byte-exact on both tiers for
   every new accept entry, against a golden generated from the dev
   tier.
2. `tsc` reports zero errors over the whole accept corpus, with the
   configuration unchanged. This is what makes 103.1 rule 5, and
   103.5's `[...map]` item, checkable rather than asserted.
3. The fused construction allocates no more than the manual
   `for…of` plus `add` loop it replaces, verified through
   `subscript_rt_ctx_live_allocations` as §14.5 verifies `for…of`,
   with a control that fires. *(Amended 2026-09-10. This rule read
   "no allocation … beyond the `Set`'s own storage", which a
   `string` source makes false: the traversal allocates one string
   handle per code point, and a duplicate the `Set` drops leaves its
   handle live. The manual loop allocates the same handles, so the
   comparison is the measurable claim.)*
4. The round **reports which goldens and counted totals moved, and
   why**. It does not assert that none moved.
5. `tools/gate.sh full` green in both profiles.

### 103.8 Three form defects, each raised twice

CLAUDE.md sets the limit: a review raises a class, a round fixes it,
and a second raise makes the class a defect of the form. Phase Review
rounds 1 and 2 of this section each raised the three below. The rules
here replace the named-site fixes.

1. **A mechanism has one owning rule.** Every other site cites that
   rule by id and states no mechanism of its own. Round 1 closed four
   prose sites that described the `new Set(source)` lowering as an
   `add` loop; round 2 found two more, one of them normative
   (`stdlib.md` §10.4). Nothing checks a prose statement of a
   lowering against the code, so restatement is the defect and not
   the wording. 103.1 rule 4 owns this lowering.
2. **A pre-registered exit criterion names a measurement. It never
   states the answer.** 103.7 predicted that two reject entries
   change no behaviour, and landing them moved a diagnostic for every
   program that reaches those two sites. Core principle 7 asks the
   spec to name "the measurement that would kill or pass it"; a
   predicted result is not one.
3. **Cite a numbered item by its subject, not by its ordinal.** Two
   amendments inserted items into 103.5, and two citations of "item
   4" then named a different item while still reading as true. An
   ordinal is a position; the subject is the fact.
