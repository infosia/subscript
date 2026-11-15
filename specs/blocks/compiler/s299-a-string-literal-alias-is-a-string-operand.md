<!-- §299 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 299. A string-literal alias is a `string` operand

*(Added 2026-11-15.)* Origin: §259.4 items 2 to 4. On 2026-11-15 the
owner selected this item. On the same day the owner decided that a
comparison with `string`, a member of `string`, and an operator with a
`string` operand convert the alias value to `string`. This changes the
scope of §259, which the owner set on 2026-11-13.

Problem, measured at `349a2129` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `type Mode = "a" | "bb";`,
`type Wire = CEnum<{ low: 3; high: 7; }>;`, `m: Mode` holds `"bb"`,
`w: Wire` holds `"high"`, and `s: string` holds `"bb"`. `tsc` accepts
each form with no error. The checker rejects each one.

| Form | Checker |
|---|---|
| `const xs = [m, "c"];` | S100 "the array element expects `Mode`, got `string`" |
| `m as string` | S100 "`as` converts between sized numerics … cannot convert `Mode` to `string`" |
| `m === s`, `s !== m`, `w === s` | S100 "operator `===` not defined for `Mode` and `string`" |
| `m.length` | S100 "`Mode` has no member `length`" |
| `m.toUpperCase()` | S100 "`Mode` has no method `toUpperCase`" |
| `"x" + m`, `m + "x"`, `m + m` | S100 "operator not defined for `string` and `Mode`" |
| `acc += m`, `acc: string` | S100 "operator `+=` is not defined for `string` and `Mode`" |
| `switch (s) { case m: … }` | S100 "the case label expects `string`, got `Mode`" |

§259 converts an alias value to `string` at a place that expects
`string`. The forms above read the value as a `string` too: the other
operand, the member, or the operator is of `string`. The conversion of
§259 rule 3 makes no allocation.

### 299.1 Rules

1. A value of a string-literal alias, plain (§24) or wire (§50),
   converts to `string` (§259 rules 2 and 3) at each of these places:
   1. An operand of `===` or `!==` whose other operand has the type
      `string`, `string | null`, or `string | undefined`. The
      comparison is the comparison of two strings.
   2. An operand of `+` whose other operand is a `string` or a value of
      a string-literal alias, and the right operand of `+=` whose
      target is a `string`. The result is a `string`.
   3. The receiver of a member of `string`: a property or a method.
   4. The operand of `as string`.
   5. A `case` label of a `switch` whose subject is a `string`.
   6. An element of an array literal with no contextual type, if
      another element of that literal is a `string` or a string
      literal, a member literal included. The element type is
      `string`: `tsc` widens a literal element, so `[m, "a"]`,
      `["a", m]`, and `[m, "c"]` each have the type `string[]`.
2. These keep §24.1, with no conversion: `===` and `!==` between two
   values of the same alias, and between an alias value and a literal
   operand, and a `case` label that is a literal operand. They lower to
   an integer compare (§259 rule 6). A literal operand that is not a
   member keeps its present diagnostic (`tsc` gives TS2367 or TS2678).
2a. A *literal operand* is a string literal, a template with no
   substitution, a parenthesized literal operand, or a read of a
   `const` binding, local or module, that has no type annotation and
   whose initializer is a literal operand. `tsc` gives each of them a
   string literal type. A `const` with a type annotation has the
   declared type: `const c: string = "zzz"` is a `string` (rule 1). Another
   declaration that `tsc` gives a literal type (a `readonly` field, an
   `as const` value, a function with a literal result type) is not a
   literal operand here, and is recorded in §299.4.
3. These stay rejected: a `string` where an alias is expected, an alias
   where another alias is expected, an operator between two different
   aliases, and `Mode[]` where `string[]` is expected (§259 rule 5).
   `m < s` stays rejected, as `<`, `<=`, `>`, and `>=` on two strings
   are (C24, row 31); `tsc` accepts
   it.
4. An array literal with no contextual type whose elements are values
   of one alias only keeps the alias element type. An array literal
   with a contextual type keeps §212 and §259.
5. No program that `tsc` rejects changes from rejected to accepted. The
   development tier, the ship tier, and the interpreter give the same
   output. The accept entry carries `js-comparable` and equals `node`.

### 299.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a458-a-string-literal-alias-is-a-string-operand` holds each form of
   the table, for a plain and a wire alias, and is rejected. The
   tracking note records the output.
2. After the change, `a458` passes `tsc`, the three tiers, and `node`.
   It prints each result.
3. A unit test pins rules 2, 2a, and 4: `m === "a"`, `m === m2`, and
   `m === c` with `const c = "a"` have no string comparison in the
   emitted C (§24.4 item 3); `m === c` with `const c = "zzz"`, and
   `` m === `zzz` ``, keep the diagnostic of a non-member; `[m, "a"]`
   and `["a", m]` have the type `string[]`, and `[m, m2]` has the alias
   element type. A control in the same test: `m === s` has a string
   comparison.
4. The implementation commit changes the Q32 revision paragraph in
   `specs/blocks/collisions.md`, C24 row 30 (a member
   of a literal alias), and the §259 line of §24.1 to cite this
   section.
5. `main` of `a458` is not async and not a generator, so the LIR text
   snapshot does not change. No golden moves. If a corpus entry or a
   golden changes its type or output, report it and stop.
6. The full gate passes.

### 299.3 Phase Review

Two fresh reviews, each by measurement against `tsc`. No CRITICAL
finding.

Before the first round ended, Claude removed `<`, `<=`, `>`, and `>=`
from rule 1.1: C24 row 31 gives no lowering for an order of two
strings. The coding agent removed the string order function that the
first round had added.

The first review:

1. MAJOR, realistic, fixed: `["a", m]` had the type `Mode[]`; `tsc` and
   the contract commit give `string[]`. Rule 4 rested on a `tsc` type
   that the measurement contradicts: `tsc` gives `string[]` to
   `[m, "a"]` too. Rules 1.6 and 4 now follow `tsc`.
2. MAJOR, realistic, fixed: `const c = "zzz"; m === c` was accepted and
   gave `false`; `tsc` gives TS2367. The same for a wire alias, a
   template with no substitution, and a `case` label. Rule 2a now names
   a literal operand.
3. MINOR, fixed: a comment cited a wrong rule number.
4. MINOR, fixed: the member-literal rewrite repeated `check_lit` and
   held panics in library code. It is removed.
5. MINOR, fixed: a test asserted "any S100"; it pins each message now.
6. MINOR, fixed: the citation form of C24 row 30.

The second review:

1. MAJOR, realistic, fixed: an annotated `const`, such as
   `const c: string = "zzz"`, counted as a literal operand, so
   `m === c` was rejected; `tsc` accepts it. Rule 2a now excludes a
   binding with a type annotation.
2. MINOR, realistic, recorded: §299.4 item 2.

The first full gate failed in `generic_forms_follow_tsc`: the omitted
instance set changed from 7815 to 7814. Rules 1.4 and 1.5 accept
`(x) as string` and `switch (s) { case (x): }`, and `tsc` accepts each.
The divergence `AliasCaseNonLiteral` now cites §299, and the matrix
record of two cells still cited §41. With the record moved, the set is
7813: the non-literal case label on an alias subject is a recorded
divergence cell of the matrix.

### 299.4 Open

1. A declaration that `tsc` gives a literal type and that is not a
   literal operand of rule 2a (a `readonly` field, an `as const` value,
   a function with a literal result type) is a `string` operand here.
   `m === K.Z`, with `static readonly Z = "zzz"`, is accepted and gives
   `false`; `tsc` gives TS2367.
2. `let c = "zzz"; switch (m) { case c: }` stays rejected with the
   divergence `AliasCaseNonLiteral`. `tsc` accepts it. This is older
   than §299: rule 1.5 covers a `string` subject only.
