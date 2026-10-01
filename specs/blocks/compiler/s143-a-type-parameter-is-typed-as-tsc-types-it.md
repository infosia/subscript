<!-- §143 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 143. A type parameter is typed as `tsc` types it

*(Added 2026-10-01.)* Origin: §135.3; owner decision of the same day
to close the gap.

Problem: §135 rule 2 keeps a diagnostic of the opaque check only from
a closed list of kinds, and leaves every other decision to the
per-instance check. So the checker accepts a generic body that `tsc`
rejects when the program does not instantiate the body, or when every
instance accepts the form. That breaks CLAUDE.md invariant 5. Measured
at `51bf01a5` through `subscript check`, against `tsc` 5.9.2 with the
`tsc_corpus.rs` options and `prelude/lang.d.ts`:

| Program | `subscript check` | `tsc` |
|---|---|---|
| `function gf<T>(x: T): boolean { return x > 1; }` with `gf<i32>(3)` | no errors | TS2365 |
| `function gf<T extends Box>(b: Box): i32 { const t: T = b; return t.v; }` with `gf<Box>(new Box())` | no errors | TS2322 |
| `class G<T> { v: i32; w: T \| null = null; }`, no instance | no errors | TS2564 |
| `function gf<T>(x: T): i32 { x.v = 3; return 0; }`, no instance | no errors | TS2339 |

`specs/tracking/s135-opaque-generics.md` (Residual gap) lists the other
measured forms.

The form of §135 is the cause. The opaque type is an empty reference
class, so it carries no constraint and no `tsc` typing. Each site that
must not reject a `tsc`-valid form asks `defers_to_instance` and makes
up a result type. A site that nobody patched rejects a `tsc`-valid
program, and a site that defers accepts a `tsc`-invalid one. No list of
sites is total. The form must carry the typing of a type parameter, and
a check must compare every form against `tsc`.

### 143.1 Rules

1. In the opaque check (§135 rule 1), a type parameter is a type of its
   own, with its constraint. It is typed as `tsc` 5.9.2 types it:
   a. A member read, a member write, a method call, a call of the
      value, an operator, an index, an iteration, and a narrowing on a
      value of type `T` use the apparent type of `T`: its constraint,
      or no member and no operation beyond what `tsc` allows on an
      unconstrained type parameter.
   b. A value is assignable to `T` only if its type is `T`. `T` is
      assignable to its constraint, and to each type that its
      constraint is assignable to.
   c. The result type of an operation that involves `T` is the type
      `tsc` gives (for example `number` for `-x`, `boolean | T` for
      `b || x`). A `number` result is not a sized type: every site
      that reads it (an operator, an index, an equality, an assignment)
      reads `number`, and a diagnostic prints `number`.
   d. A constraint that names another type parameter resolves through
      that parameter's own constraint, whatever the declaration order.
   Where this list and `tsc` differ, `tsc` decides. The total check of
   rule 4 measures it.
2. The opaque check reports every diagnostic that it produces. §135
   rule 2's closed list of kept kinds retires. A diagnostic that does
   not involve a type parameter (for example a field with no
   initializer in a generic class) is reported whether or not the
   program instantiates the body.
2a. One kind of decision stays with the per-instance check: a language
   restriction of this project that depends on which type the argument
   is, where `tsc` accepts the form on `T` (for example `${x}` with
   `x: T`, or a `T` field of a `@ValueType` class). The opaque check
   accepts such a form, with the result type `tsc` gives. The
   implementation names each such restriction in one list. The total
   check of rule 4 reports a restriction that the list does not name.
   A deferral skips only this project's restriction. At the same site
   the typing of rule 1 applies first: the opaque check rejects the
   form where `tsc` rejects it on the apparent type, and defers only
   what remains. *(Added 2026-10-01 after the Phase Review: every entry
   of the list deferred on "the type involves `T`" alone, so a deferral
   also skipped the `tsc` typing at its site — `s += x` and `if (cb)`
   on a function of `T` passed where `tsc` gives TS2365 and TS2774.)*
3. §135 rules 1, 2b, and 3 stay. Rule 3 applies to every diagnostic of
   rule 2.
4. A total check runs in the gate. It is a matrix of generic forms.
   Each cell is one form on one kind of type parameter, with no
   instance or with one instance that the per-instance check accepts.
   The kinds are: no constraint, a class constraint, a numeric
   constraint, an array constraint, and `T | null` with a class
   constraint. The forms include every form of §135.3, every form of
   the Residual gap table, and every regression form of the two §135
   Phase Reviews (`switch (x)`, `for...of` over a constrained `T`, a
   constrained `T` as an index, narrowing of a constrained `T`, `??`
   with a `T | null` operand, a fresh `number` beside a `number`).
   The check runs `tsc` once over all cells and `subscript check` on
   each cell, and fails on each cell where:
   a. `subscript check` accepts and `tsc` rejects (invariant 5); or
   b. `tsc` accepts and `subscript check` rejects, and the cell names
      no rule code of this project. A cell that names a code also names
      the record that decides the restriction: a collision id, or a
      section of this contract, as a reject entry's `questions` line
      does. The record must state the restriction; a cell that names a
      record that does not state it fails.
   The test states its measured cost.
5. Measure first: the round builds the form of rules 1 and 2 before
   the Red entries land. It runs every corpus accept, warn, and trap source
   and every example, and records each one that becomes rejected, with
   the diagnostic. It records the number of failing matrix cells and
   the checker time on the corpus against the pin. If any corpus
   source or example is rejected, the round stops and reports. The
   owner decides.
6. §135.3 and the generic-body item of `collisions.md` §3 retire when
   the matrix of rule 4 has no failing cell.

### 143.2 Acceptance

1. Red first: one reject entry for each program of the Problem table,
   each accepted at `51bf01a5`, each header with the measured `tsc`
   code.
2. The matrix test of rule 4 passes, and states its measured cost.
   A unit test builds one cell of each failure kind (4a and 4b) and
   shows that the check reports it.
3. Every corpus accept, warn, and trap source and every example stays
   accepted.
4. No `.expected` golden moves.
5. Each public item that the change adds has a direct unit test.
6. `specs/tracking/s143-type-parameter-typing.md` records the rule 5
   numbers, the list of rule 2a, the matrix size and cost, and the
   checker time against the pin.

### 143.3 Open

These items are MINOR under CLAUDE.md invariant 6. They do not block
§143.

1. A waived diagnostic that carries no divergence tag matches its record
   by a message fingerprint. A short fingerprint matches messages that
   the record does not state: the §97 fingerprint `null` matches every
   untagged S100 that prints the type `null`, so rule 4b checks nothing
   for the `null` kind. The form change is a divergence tag at the
   emitter of every waived restriction, so a record matches by id only.
2. The `map` element-kind diagnostic (`compiler/src/check/expr/method.rs`)
   builds the array spelling by hand and prints `(i32) => i32[]` for an
   array of functions.
3. A condition must be `boolean`: `const n: i32 = 3; if (n) {}` gives
   S100 "condition must be boolean", and `tsc` accepts it. No collision
   record states this rule. The matrix cites §68, which states an IR
   fact, not the source rule.
