<!-- §330 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 330. Two different literals do not compare

*(Added 2026-11-18.)* Origin: §156.3, rank 9 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `d65ed74b`. `tsc` gives a literal type to a
literal, and to a declaration whose initializer is a literal with no
type annotation. It rejects an equality of two literal types that do
not overlap. subscript accepts each form below, and the program prints
`false` (invariant 5: an accepted program must pass `tsc`):

| Form | `tsc` |
|---|---|
| `const MODE = 1;` then `MODE === 2`, `MODE !== 2`, `MODE == 2` | TS2367 |
| `const MODE = -1;` then `MODE === 1` | TS2367 |
| `const MODE = 1; const OTHER = 2;` then `MODE === OTHER` | TS2367 |
| `const A = 1.5;` then `A === 2.5` | TS2367 |
| `const F = true;` then `F === false` | TS2367 |
| `const S = "a";` then `S === "b"` | TS2367 |
| `1 === 2` | TS2367 |
| `readonly size = 4;` then `k.size === 5`; `static readonly Z = 0;` then `K.Z === 1` | TS2367 |
| `const z = K.Z;` then `z === 1` | TS2367 |
| a local `const x = 3;` then `x === 4` | TS2367 |
| `switch (MODE) { case 1: … case 2: … }` | TS2678 at `case 2` |

`tsc` accepts these, and subscript accepts them: `const MODE: i32 = 1`
(an annotation gives the declared type), `let m = MODE` (a `let`
widens), `MODE === 1 || MODE === 2`, `if (MODE === 1) … else …`,
`const MODE = 0x10;` then `MODE === 16`, `const A = 1e2;` then
`A === 100`, and `MODE < 2`.

The string case of a string-literal alias value is §299 rule 2. The
`readonly` field store is §316 rule 4a.

### 330.1 Rules

1. **A literal operand.** §299 rule 2a and §316 rule 1 define a
   literal operand of a string. The same definition holds for a number
   and a boolean: a numeric literal, a unary `-` of a numeric literal,
   `true`, `false`, a parenthesized literal operand, a read of a
   `const` binding (local or module) with no type annotation whose
   initializer is a literal operand, and a read of a `readonly` field
   of §316 rule 1. Its value is the value of the literal: `0x10` and
   `16` are one value, and `1e2` and `100` are one value.
2. **Equality.** If both operands of `===`, `!==`, `==`, or `!=` are
   literal operands of one kind (string, number, or boolean) and their
   values differ, the comparison is rejected. The text names both
   literals. The site is `TscRejects` (TS2367).
3. **`switch`.** If the subject of a `switch` is a literal operand, a
   `case` label that is a literal operand of the same kind with another
   value is rejected. The text names both literals. The site is
   `TscRejects` (TS2678).
3a. **Narrowing.** `tsc` narrows a literal binding. On a path where a
   test of the binding against its own value is false, the binding has
   the type `never`, and `never` compares with any value
   (`MODE === 1 || MODE === 2`, the `else` of `if (MODE === 1)`, a
   `default` after `case 1`). This section does not model the paths.
   A comparison of rule 2 is not rejected if an equality operator or a
   `case` label that compares the same binding with its own value
   comes before it in source order, in the same body or an enclosing
   body. A `case` label of rule 3 is not rejected if such an equality
   operator comes before the `switch`. A `case` label of the same
   `switch` does not count. So `switch (MODE) { case 1: … case 2: … }`
   stays rejected, as `tsc` rejects it. The rule errs toward
   acceptance: `if (MODE === 1) { … }` then `MODE === 2` is accepted,
   and `tsc` rejects it (TS2367, measured). *(Added 2026-11-18: round 1
   stopped because rule 2 rejected the right operand of
   `MODE === 1 || MODE === 2`, which `tsc` accepts.)*
4. **Nothing else changes.** A value with a type annotation, a `let`, a
   parameter, a call result, and an operator result are not literal
   operands. Flow narrowing does not make a literal operand. `<`, `<=`,
   `>`, and `>=` do not change. An equality of two literal operands
   with the same value stays accepted. §299 rule 2 keeps its alias
   compares.

### 330.2 Acceptance

1. Red first: a reject entry for the equality forms of the problem
   table, and one for the `switch` form. Each header states the `tsc`
   code, measured. At the pin, `check` accepts each.
2. An accept entry with each accepted form of the problem table and an
   equality of two literal operands with the same value
   (`const MODE = 1; MODE === 1`). It also holds the rule 3a forms that
   `tsc` accepts: the `else` of `if (MODE === 1)`, a return after
   `if (MODE === 1)`, `MODE !== 1 && MODE === 2`, a `switch` after
   `if (MODE === 1) return;`, and a `default` after `case 1`. It prints
   each result. It is `js-comparable: yes` if `node` prints the same
   bytes.
3. A unit test for rules 1 to 3a, each with a control in the same shape
   that stays accepted or stays rejected (core principle 9).
4. The CLI `check` output of each other corpus entry is the same at the
   pin and after. The development tier and the ship tier print the
   same output.

### 330.3 Sections this one amends

- §156.3: the literal type of a numeric or boolean `const` closes for
  equality and `switch`.
- §299 rule 2a: a literal operand is also a number or a boolean
  literal, and rule 2 of this section applies to two literal operands
  of one kind.
