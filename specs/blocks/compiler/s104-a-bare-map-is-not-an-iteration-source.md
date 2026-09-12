<!-- §104 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 104. A bare `Map` is not an iteration source

Origin: §103.5's `[...map]` item. The measurement that produced it
named the spread. The same hole is in `for…of`, which is the form
programs actually use.

Measured 2026-09-10 at `3d03f80`, both production tiers, against
TypeScript 5.9.2:

| form | this language | stock `tsc` |
|---|---|---|
| `for (const k of m)`, with `const n: i32 = k` | binds `K`, runs | `TS2322`, `[number, string]` is not `number` |
| `const ks: i32[] = [...m]` | `K[]`, prints the keys | `TS2322`, `[number, string][]` is not `number[]` |
| `for (const k of m.keys())`, with `const n: i32 = k` | binds `K` | accepts |
| `const xs: i32[] = [...set]` | `K[]` | accepts |

**Invariant 5 says every accepted program type-checks under stock
`tsc`.** These two forms accept programs that do not. That is not a
divergence: a divergence describes different behaviour on a program
both systems accept, and `collisions.md` cannot record a form `tsc`
refuses to compile.

Q30 chose "bare `Map` iterates keys, as `keys()` does". The choice is
internally consistent, and it is the choice that breaks the invariant,
because TypeScript defines a `Map` as `Iterable<[K, V]>`.

**The corpus did not catch it, because neither entry types the bound
value.** `a77` interpolates `key`, which `tsc` accepts for a pair too.
`a81` binds `[...map]` with no annotation, so `tsc` infers
`[i32, string][]`, the language infers `i32[]`, and neither reports.
Both entries are `tsc`-clean by accident, and an accident is not a
gate.

**The rule change is forced.** Three answers exist. Yielding pairs
needs a tuple type, which is the gap that keeps `entries()` and
`new Map([[k, v]])` out. Weakening invariant 5 contradicts a permanent
invariant, and editor tooling is what invariant 5 buys. Rejecting the
bare `Map` is the remaining answer, and it needs no new machinery.

### 104.1 The rule

1. **`Map<K, V>` is rejected as a `for…of` subject and as an
   array-literal spread operand**, at S014.
2. **The rejection does not read how the bound value is used.** The
   annotated and the unannotated forms are both rejected. A rule that
   accepted `for (const k of m) { print(...) }` and rejected the same
   loop with `const n: i32 = k` would make acceptance depend on a
   later statement, and `a77` proves a program can avoid the
   annotation by accident.
3. **`m.keys()` and `m.values()` as a direct `for…of` subject are
   unchanged**, and `Set`, `T[]`, `FixedArray<T, N>`, `string`, and
   `Generator<T>` are unchanged in both positions.
4. The check reads the resolved subject type, as §103.2 rule 1
   requires of the view rules.
5. `new Set<K>(map)` stays rejected. §103.1 rule 5 already rejects it
   under invariant 5, and this section does not change its reason.

### 104.2 What this retires, and the path back

**This retires a capability**: the one-expression spelling for the
keys of a `Map`. The supported replacement is a loop:

    const keys: i32[] = [];
    for (const key of map.keys()) {
      keys.push(key);
    }

`[...m.keys()]` is **not** the replacement. Measured: it is S014,
because §14.1 accepts a view only as a direct `for…of` subject.

A view in spread-operand position fuses exactly as a view in subject
position fuses, and creates no escaping value, so restoring the
one-expression spelling is a small separate contract. **It is not in
this section**, and this section does not establish its cost. Recorded
so that the capability has a named path back rather than a silent
loss.

### 104.3 The diagnostic states the right fact

The diagnostic must say that this language binds `K` where TypeScript
binds `[K, V]`, so an accepted program would fail the `tsc` gate.

**It must not say that `tsc` rejects a `Map` loop.** Stock `tsc`
accepts `for (const entry of m)`. The incompatible element type is
the failure, not the loop.

### 104.4 Six diagnostics still carry a reason §103.3 retired

`compiler/src/ambient.rs` states, at six sites, that a view elsewhere
"would create a stateful iterator value that outlives its call".
§103.3 retired that reason: `compiler.md` §70 landed a
reference-counted handle, and a `Generator<T>` already outlives the
call that made it. The obstacle is a missing view type plus §70.1
decision 1's scope.

Under §103.8 rule 1 the owning rule is `stdlib.md` §14.3. The
diagnostics state the current obstacle, or they state none and point
at the rule. A user-facing message that gives a retired reason is the
defect §103 exists to remove.

**A rejection states one reason.** *(Amended 2026-09-10, after the
round fixed the six messages and the review found a seventh string.)*
A site with both a message and a `Divergence` `why` has **two**
user-facing reason strings, and nothing checks that they agree.
`Divergence::IteratorTemporary`'s `why` still read "A held iterator is
a stateful value that outlives its call" while the message beside it,
in the same diagnostic, gave the view-type obstacle.

Closing named sites does not converge, so this needs a **total
check**: a test holds the retired phrases this contract names, and
asserts that no user-facing string in the compiler contains one. It
reports every remaining site at once. The first entry is "outlives its
call".

**"In the compiler" is the `src` tree of each compiler crate**, plus
the rendered diagnostics of the reject corpus and every diagnostic
table value. *(Stated 2026-09-10 after the Phase Review measured the
boundary.)* It does not reach `prelude/lang.d.ts`, `docs/`,
`README.md`, or a `build.rs`. Measured on the day it landed:
`git ls-files | xargs grep -l "outlives its call"` names only this
contract, its tracking note, and the test. A round that widens the
boundary states what it added.

**Each part of the sweep carries its own non-empty guard.** A part
that reads nothing passes silently otherwise, which is the
firing-control defect two earlier reviews already raised. *(The sweep
has three parts — the diagnostic tables, the rendered reject corpus,
and the source literals. An earlier draft said "half".)*

### 104.4a Make the bare-`Map` path unreachable, not guarded

*(Added 2026-09-10, from the round's own report.)* CLAUDE.md: a fix
that closes named sites does not converge; make the class unreachable,
or make a total check report every remaining site at once.

`Type::iteration_element` still answers `MapKeys` for a `Map`. Three
call sites reject a `Map` before they call it, so the guard is at the
callers. **A fourth consumer reopens the invariant-5 hole and nothing
reports it.** Remove the arm, so no consumer can obtain it.

`SpreadKind::MapKeys` and `IterKind::MapKeys` then have no producer.
Three tiers keep an arm each for a shape the checker can no longer
build. Remove those with the producer.

### 104.5 Sites

- `compiler/src/check/stmt.rs`, the `for…of` subject check.
- `compiler/src/check/expr.rs`, the array-literal spread check.
- `compiler/src/ambient.rs`, §104.4's six messages.
- `specs/blocks/stdlib.md` §14.1, §14.4, §14.5.
- `specs/blocks/collisions.md` Q30.
- `specs/tracking/js-api-sweep.md`, the iteration table.
- `compiler/tests/corpus_reject.rs`, one line per new reject entry.
- `corpus/accept/a77` and `corpus/accept/a81`, and their goldens.

### 104.6 Corpus and gate (pre-registered exit criteria)

**Accept.** `a77` and `a81` move to the supported spellings. `a81`'s
`Map` segment becomes the §104.2 loop, in that entry or in one of its
own. The round reassesses both `js-comparable` headers, because the
`Map` divergence they name disappears with the form.

**Reject**, each at a pinned position with its rule code: a bare `Map`
as a `for…of` subject, and a bare `Map` as an array-literal spread
operand. **Both entries are the unannotated form**, both are
`tsc: accepts`, and **both sites carry a divergence variant**, so both
render the §79 block.

*(Amended 2026-09-10. This paragraph asked for one **typed** entry as
well. §79 rule 6 now records why that cannot be: one site serves both
`tsc` classes, its variant is fixed, and a `tsc: rejects` entry at a
site with a variant breaks §79 rule 4. Taking the variant off a site
to host that entry leaves the user-facing rejection with no
explanation, which is worse. The typed form's `TS2322` is pinned by a
unit test instead.)*

**A test runs `tsc` on the typed form and compares its code**, beside
the corpus gate that already runs it. It does not hold the code as a
literal beside the assertion, and it is not a unit test — the two
words named different test kinds for one fact until 2026-09-10. *(Amended
2026-09-10. This paragraph said a unit test "records" the code, and a
round wrote `let tsc_code = "TS2322"; … assert_eq!(tsc_code,
"TS2322")`, which cannot fail. §79 rule 6 routes the typed form's fact
here, so this is the only pin it has, and core principle 9 governs
it.)* `compiler/tests/tsc_corpus.rs` already runs `tsc` **as a
subprocess** over every corpus header; the check belongs there,
because that is where a real comparison is available. *(Corrected
2026-09-10: this sentence said "in process", and there is no
in-process TypeScript.)*

**Gate.**

1. The standing differential gate is byte-exact on both tiers for
   every moved accept entry, against a golden generated from the dev
   tier.
2. `tsc` reports zero errors over the accept corpus, configuration
   unchanged.
3. **The round demonstrates that each new reject entry fails against a
   binary built from this section's pin**, and records the diagnostic
   it gave there.
4. The round reports which goldens and counted totals moved, and why.
   It does not predict them (§103.8 rule 2).
5. `tools/gate.sh full` green in both profiles.
