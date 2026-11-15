# Tracking: a string-literal alias is a `string` operand (§299)

Contract: `specs/blocks/compiler/s299-a-string-literal-alias-is-a-string-operand.md`.
Origin: §259.4 items 2 to 4. On 2026-11-15 the owner selected the item
and decided that a comparison with `string`, a member of `string`, and
an operator with a `string` operand convert the alias value.

## Red

At the contract commit `038cafc2`, `subscript check` of `a458` exited 1
with 31 diagnostics. Each form of the contract table gave its S100.

## Contract change during the round

The first contract listed `<`, `<=`, `>`, and `>=` in rule 1.1. C24 row
31 gives no lowering for these operators on two strings, so `m < s`
stays rejected as `s < t` does. Claude changed the contract while the
first round ran. The coding agent had added a string order function to
the runtime and the three tiers; the second round removed it. The
coding agent also measured that `tsc` (strict) rejects an order
comparison with `string | null` (TS18047) and `string | undefined`
(TS18048).

## Form

- `compiler/src/check/string_operand.rs`: one policy decides where an
  alias value converts to `string`. The operators, `+=`, the members of
  `string`, `as string`, `case` labels, the array literal element type,
  and the §259 value places read it. The lowering is the §259
  conversion, so no runtime or backend change remains.
- The rejection sites `LiteralAliasMember` and `LiteralAliasMethod` and
  their witnesses are removed: a member of `string` on an alias value
  is accepted.
- `a458` output: 32 lines, equal under the CLI, `node`, and the hand
  trace.
- `specs/blocks/collisions.md`: the Q32 revision and C24 row 30 cite
  §299. §24.1 cites §299.

## Phase Review

Two fresh reviews, each by measurement against `tsc`: three MAJOR and
five MINOR findings; see §299.3. Each MAJOR changed the contract before
its fix:

- Rules 1.6 and 4: `tsc` gives `string[]` to `[m, "a"]` and to
  `["a", m]`. The first contract kept the alias element type for
  `[m, "a"]`.
- Rule 2a: `const c = "zzz"; m === c` is TS2367 under `tsc`, because a
  `const` with no annotation has a literal type. The first fix also
  read an annotated `const` as a literal; `tsc` gives
  `const c: string = "zzz"` the type `string`.

Measured at the end, with the debug CLI and `tsc` 5.9.2: `m === K.Z`
with `static readonly Z = "zzz"` prints `false`, and `tsc` gives TS2367
(§299.4 item 1).

The first full gate failed in the generic `tsc` matrix: the omitted set
moved from 7815 to 7813 after the record of `AliasCaseNonLiteral`
followed its new citation of §299; see §299.3.
