<!-- §146 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 146. A conditional joins its branch types

*(Added 2026-10-02.)* Origin: a downstream proposal (`HANDOFF.md`,
2026-10-02, item 1). Owner decision of the same day: take the four
proposals in order; this is the first.

Problem: with no contextual type, §45.2 types a conditional by its then
branch and requires the else branch to be assignable to it. So the
branch order decides acceptance, and a nullable result needs an
annotation. Measured at `de41409f` through `subscript check`; `tsc`
5.9.2 accepts every line:

| Form (`flag: boolean`, no annotation) | `subscript check` |
|---|---|
| `flag ? new Box() : null` | S100 "the else branch expects `Box`, got `null`" |
| `flag ? null : new Box()` | S100 "the else branch expects `null`, got `Box`", and S100 "cannot infer a type from `null`" |
| `flag ? new Box() : b` with `b: Box \| null` | S005 "the else branch expects `Box`, got `Box \| null`" |
| `flag ? f : null` with `f: () => i32` | S100 "the else branch expects `() => i32`, got `null`" |
| `flag ? (flag ? new Box() : null) : null` | S100 twice |

The annotated form `const v: Box | null = flag ? new Box() : null` is
accepted (§45.2).

### 146.1 Rules

1. With no contextual type, the type of `c ? a : b` is the join of the
   two branch types, the same in either branch order:
   a. two equal types join to that type;
   b. `T` and `null`, and `T` and `T | null`, join to `T | null` when
      `T | null` is a legal type (C7: a reference, a function type, a
      handle, or another nullable-capable type);
   c. every other pair has no join and is rejected, as before §146:
      two different classes (C7 has no general union), a scalar and
      `null` (C7), and two numeric widths (no implicit conversion, C3).
      The diagnostic names both branch types.
2. With a contextual type, §45.2 stands: each branch is checked
   against the context.
3. The evaluation order and the single evaluation of the selected
   branch do not change.
4. A conditional whose join is `T | null` narrows like any other
   `T | null` value.

### 146.2 Acceptance

1. Red first: an accept entry with each form of the Problem table (both
   branch orders, a nullable operand, a function value, a handle from
   the interop fixture if the corpus can reach one without the fixture
   harness, and a nested conditional), printing which branch ran and
   narrowing the result. It is rejected at the contract pin; `node`
   output equals the golden where the entry is js-comparable.
2. Reject entries for rule 1c: two different classes, `i32` and `null`,
   `i32` and `f64`; each header records the measured `tsc` result.
3. `r119-conditional-without-context` pins the old directional rule with
   `flag ? new BranchValue(7) : null`, a form that rule 1b accepts. It
   retires (`retired:r119-conditional-without-context`).
4. The §143 matrix: no cell changes verdict except those that §146
   accepts; each such cell is listed in the tracking note.
5. No existing `.expected` golden moves.
