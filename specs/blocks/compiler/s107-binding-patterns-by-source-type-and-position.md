<!-- §107 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 107. Binding patterns, by source type and position

Origin: `REPORT.md` item B. Every pattern form answers S100
"destructuring is not in the decided surface", the word
`destructuring` appears in no file under `specs/blocks/`, and §79
rule 4 therefore has no id for a reject entry. A rejection with no
record is what §103 exists to remove.

**One record for all patterns does not survive this project's own
rule**, because the forms have different blockers. An array pattern
over a `T[]` needs no tuple type and no object type. A pair pattern
over a `Map` needs a tuple. An anonymous object source needs an object
type. §107 decides them apart.

Measured 2026-09-10 at `3d03f80`, TypeScript 5.9.2, node v24.18.0.
Every form below is `tsc`-clean: `const [a, b] = xs`, `const [, b] =
xs`, `const { x } = p`, `const { x: renamed } = p`,
`function take([a, b]: i32[])`, `function field({ x }: P)`.

**Each one also emits a cascade today**: S100 at the pattern, then one
S016 `unknown name` per name the pattern would have bound. §107.4
forbids that.

### 107.1 Accepted

1. **An array binding pattern over a `T[]` or a `FixedArray<T, N>`**,
   in a **local** `const`/`let` declaration, in a **parameter**, and
   in a **`for…of` binding** whose element type is one of those two.
   "Parameter" is four positions: a free function, a method, a
   **constructor**, and a lambda. Each has its own checker path.
   *(Corrected 2026-09-11 by the implementation round, which
   enumerated the sites: the word "local" was missing, and the
   constructor parameter was unnamed. §107.1 accepts "a parameter"
   and §107.3 holds no reason to refuse the constructor, so it is
   accepted.)*
2. **A named-field pattern over a reference or value class**, in the
   same three positions, with and without renaming
   (`const { x: renamed } = p`).

*(The `for…of` binding was added 2026-09-10, before implementation.
The first draft named the declaration and the parameter only, so
`for (const [a, b] of xss)` had no rule at all — neither accepted here
nor rejected in §107.3. Measured: both loop forms are `tsc`-clean and
answer S100 today.)* The element type decides the pattern, exactly as
the declared type decides it elsewhere. A `for…of` whose **subject**
is rejected stays rejected by the subject's own rule; §104.1 governs a
bare `Map`, and the pattern rule never reaches it.

`C1` is not a blocker for rule 2. It records that this compiler
rejects **structural substitution**, and that an object literal has no
standalone type. Reading a named field from a valid class instance is
an ordinary field read.

### 107.2 Semantics

| area | rule |
|---|---|
| source | Evaluate it **once**. Bind in source order. |
| short array | An absent required element takes the **existing `index-out-of-bounds` trap**. JS binds `undefined`; this language has none. A recorded divergence. |
| extra elements | Ignored. |
| empty pattern | The source still evaluates. |
| skipped element | `const [, b] = xs` advances the position and binds no name. A skipped position past the end does **not** trap, because nothing reads it. |
| parameters | The declared parameter type decides the pattern. Extraction runs at entry, in parameter order. |
| mutability | `const` and `let` keep their existing meaning per bound name. |
| copy and ownership | Ordinary element reads, field reads, value copies, and reference ownership. This section adds none. |
| field access | Access checks hold, and a getter runs. Do not read through storage where a getter must execute. |

The short-array rule uses the **ordinary checked read**, not an eager
whole-pattern length test. An eager test moves the trap ahead of the
earlier bindings' effects, which changes what a program observes.

### 107.3 Rejected, each with its own reason

| form | reason |
|---|---|
| `for (const [k, v] of map)` | a bare `Map` is not an iteration source (§104.1). The pair form additionally needs a tuple type |
| a pattern over `entries()`, or over any tuple-typed source | no tuple type |
| a pattern over an anonymous object source | no object type |
| a default value, `const [a = 1] = xs` | it needs a rule for a missing element and for `undefined`. `null` is not an equivalent trigger, and the language has no `undefined` |
| array rest, `const [a, ...rest] = xs` | allocation and copy semantics for the rest array |
| object rest | a result shape, and property-selection rules |
| a nested pattern | recursive type checks and ordered effects |
| an assignment pattern, `[a, b] = xs` | target evaluation and write order, which a declaration-only contract does not cover |
| a **module-level** declaration, and a **mirror `declare const`** | *(Added 2026-09-11, measured, and a cost rather than a scope statement.)* A module-level name's type resolves from its annotation alone, one pass before the checker reads an index or a member, so a pattern there needs a **second derivation** of each bound name's type beside the one `check_index` and `member_on` already give. Core principle 8 forbids that. `hir::Global` also holds one initializer and no prologue, so the source temporary would become a permanent root, which §107.2's evaluate-once rule forbids |
| a **non-array, non-class** source, and a **computed field name** | The complement of §107.1. Neither is a shape §107.1 admits, and each needs its own diagnostic rather than the general one |

**Each row names work, not scope.** "Not in v1" is not a reason here,
and a round that implements any row states the cost it measured.

### 107.4 A rejected pattern reports once

A pattern this section rejects emits **one** diagnostic, at the part
of the pattern that carries the reason — the `...rest`, the default,
the nested pattern — and at the pattern's start where no one part
does. It does not then emit an S016 for each name the pattern would
have bound. *(The position was "at the pattern" until 2026-09-11; the
implementation points at the offending part, which is the better
place, and the contract follows it.)*

Measured 2026-09-10, and the count differs by position, which is why
the round fixes the cascade rather than one site:

| form | diagnostics today |
|---|---|
| `const [a, b] = xs` | 3 |
| `const { x } = p` | 2 |
| `function take([a, b]: i32[])` | 3 |
| `const f = ([a, b]: i32[]): i32 => …` | 3 |
| `class Box { sum([a, b]: i32[]): i32 }` | 3 |
| `for (const [a, b] of xss)` | 1 |

Only the last is already correct. A rejected pattern in any position
reports once.

*(The free-function row read 5 until 2026-09-11. The probe behind it
held an array pattern and a field pattern in one file, so the count
was 3 plus 2. Every other row reproduced exactly. A measurement that
mixes two forms is not a measurement of either.)*

### 107.5 Sites

- `compiler/src/check/mod.rs` and `compiler/src/check/stmt.rs`: every
  existing pattern rejection site. **The round enumerates them before
  it changes one.** *(Corrected 2026-09-11. This rule said six
  positions. The enumeration found **nine**, behind five `self.error`
  call sites. One site serves four checker paths. The three the rule
  missed: a **module-level** declaration, a **mirror `declare
  const`**, and a **constructor** parameter.)* The nine: a local
  declaration; a `for…of` binding; a module-level declaration; a
  mirror `declare const`; four parameter paths — free function, method
  and static, constructor, lambda; and an assignment target. A fix at
  the declaration reaches none of the others.
- The lowering, for the accepted forms.
- `specs/blocks/collisions.md`: the record §79 rule 5 needs for the
  reject entries. *(The round cited `compiler.md` §107.1 and §107.3
  as the ids, which §107.6 permits; no `### C<n>` heading was added.
  Q22 and Q30 gain a pointer instead — the planner's edit, since a
  round may not touch `specs/`.)*
- `specs/blocks/stdlib.md` §9 and §14, where a pattern meets a
  container. *(Also the planner's edit, made 2026-09-11 after the
  review found both unchanged.)*
- `compiler/tests/corpus_reject.rs`.

### 107.6 Corpus and gate (pre-registered exit criteria)

**Accept**, covering all three positions: an array pattern and a
field pattern in a declaration, in a parameter, and in a `for…of`
binding; a renamed field; a
skipped element; an empty pattern whose source has an observable
effect, proving the source still evaluates; a source evaluated once,
proved by an effect that would repeat; a value-type element and a
reference-type element; `let` rebinding after a pattern.

**Trap**: a short array source, at the `index-out-of-bounds` trap
§107.2 names.

**Reject**: one entry per §107.3 row, each at a pinned position with
its rule code. **The round measures each row's `tsc` class** and
applies §79 rule 6 where one site serves both. §79 rule 1 gives one
variant per **topic**, and §107.3's rows are separate topics, so the
round decides how many variants the rows need rather than assuming
one. A `collision` id may name `compiler.md` §107.3 where
`collisions.md` has no heading. One entry pins §107.4 — a rejected
pattern reporting exactly one diagnostic — and it is Red today,
because the same program now reports three.

**Gate.**

1. The differential gate is byte-exact on both tiers for every new
   accept entry, against a golden generated from the dev tier.
2. `tsc` reports zero errors over the accept corpus, configuration
   unchanged. Every accepted form in §107.1 is `tsc`-clean, measured
   above; the gate is what keeps that true.
3. `node` runs the accept entries and agrees, except where a header
   names the short-array divergence.
4. Each new accept entry, and the §107.4 entry, is Red at this
   section's pin, and the round records the diagnostic it gave there.
5. The round reports which goldens and counted totals moved, and why
   (§103.8 rule 2).
6. `tools/gate.sh full` green in both profiles.
