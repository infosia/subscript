<!-- §321 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 321. A string case label compares an alias subject as a string

*(Added 2026-11-17.)* Origin: §299.4 item 2. The owner told the work to
continue down the open-item triage on 2026-11-16.

Problem, measured at `0f327341`. With `type Mode = "aaa" | "bbb"`:

```ts
function f(m: Mode, c: string): void {
  switch (m) { case c: print("hit"); break; default: print("miss"); }
}
```

gives S100 "case labels for string-literal union alias `Mode` must be
string literals naming a member" (divergence `AliasCaseNonLiteral`).
`tsc` accepts it. With `f("aaa", "aaa")`, `f("bbb", "aaa")`, and
`f("aaa", "zzz")`, `node` prints `hit`, `miss`, `miss`. §299 rule 1.1
already converts an alias value to `string` for `===` against a
`string`; rule 1.5 converts only for a `string` subject.

### 321.1 Rules

1. A `case` label of the type `string`, `string | null`, or
   `string | undefined`, in a `switch` whose subject is a value of a
   string-literal alias, compares the subject converted to `string`
   (§259 rules 2 and 3) with the label, as `===` of §299 rule 1.1 does.
2. A label that is a literal operand (§299 rule 2a, §316) keeps §299
   rule 2: an integer compare for a member, and the present diagnostic
   for a literal that is not a member.
3. Each label is evaluated in source order, and the subject once, as
   §299 states for a `switch`.
4. No program that `tsc` rejects becomes accepted. The development
   tier, the ship tier, and the interpreter give the same output. The
   accept entry carries `js-comparable` and equals `node`.

### 321.2 Acceptance

1. Red first: an accept entry with the program above and a `switch`
   that mixes a member literal label and a `string` label, with output
   equal to `node`. It is rejected at the pin.
2. A test that a label of another alias, and a label of a number type,
   stay rejected.
3. The CLI `check` output of each corpus entry is the same at the pin
   and after the change, except the new entry. The divergence
   `AliasCaseNonLiteral` keeps each form that still reaches it, or is
   deleted with its witnesses if no form reaches it (§307 rule 4).

### 321.3 Phase Review

*(Filled in after the review.)*

### 321.4 Open

None.
