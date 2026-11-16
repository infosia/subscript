<!-- §316 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 316. A readonly literal field is a literal operand

*(Added 2026-11-16.)* Origin: §299.4 item 1. The owner selected the
soundness items of the open-item triage on 2026-11-16.

Problem, measured at `51135545`. With `type Mode = "aaa" | "bbb"`, a
parameter `m: Mode`, `k: K`, and

```ts
class K {
  static readonly Z = "zzz"; static readonly A = "aaa";
  readonly w = "zzz"; readonly b = "bbb";
  static readonly S: string = "zzz";
}
```

| Form | `tsc` | Checker |
|---|---|---|
| `m === K.Z` | TS2367 | accepted, prints `false` |
| `m === k.w` | TS2367 | accepted, prints `false` |
| `m === K.A`, `m === k.b` | accepts | accepted |
| `m === K.S` | accepts | accepted (a `string`, §299 rule 1) |
| `switch (m) { case K.A: … case k.b: … }` | accepts | S100 "case labels … must be string literals naming a member" |

`tsc` gives a `readonly` field with no type annotation and a literal
initializer the literal type of its initializer. §299 rule 2a treats it
as a `string` operand, so the first two forms are accepted against
invariant 5, and the case label is rejected. `as const` and a literal
result type are outside the subset (measured: S100), so the field is
the only declaration of §299.4 item 1 that reaches the checker.

### 316.1 Rules

1. A read of a `readonly` field, static or instance, whose declaration
   has no type annotation and whose initializer is a literal operand of
   §299 rule 2a, is a literal operand of §299 rule 2a. Its literal is
   the literal of the initializer.
2. So §299 rule 2 applies to it: against a value of a string-literal
   alias, a member literal compares as an integer compare (§259 rule 6),
   and a literal that is not a member keeps the diagnostic of a literal
   operand that is not a member (the `tsc` code is TS2367 or TS2678). A
   `case` label that reads such a field and names a member is accepted.
3. A `readonly` field with a type annotation keeps its declared type
   (`static readonly S: string` is a `string`, §299 rule 1).
4. A field of rule 1 has the literal type of its initializer, as in
   `tsc`. A store to it in a constructor must store the same literal;
   any other value is rejected, as `tsc` rejects it with TS2322. So each
   field of rule 1 holds its literal, and rule 1 applies to each, with
   or without a constructor body. A subclass is outside the subset.
   *(Amended 2026-11-16 before the review: the first round excluded the
   instance fields of a class with a constructor body, so `m === k.w`
   stayed accepted there, and `this.w = "yyy"` in a constructor was
   accepted at `51135545`; `tsc` rejects both, measured.)*
4a. Rule 4 holds for each `readonly` field with no type annotation
   whose initializer is a literal: a string, a number, or a boolean
   literal. A constructor store of another value is rejected; `tsc`
   gives TS2322 (`readonly n = 0` with `this.n = c`, measured).
   *(Added 2026-11-16 after the Phase Review: the number and boolean
   forms were accepted at `55cb2cb2`.)*
4b. A field initializer that reads another field of rule 1 through
   `this` (`readonly b = this.a`) has the literal of that field, as
   `tsc` gives it. *(Added 2026-11-16 after the Phase Review.)*
5. The development tier, the ship tier, and the interpreter give the
   same output.

### 316.2 Acceptance

1. Red first: a reject entry for `m === K.Z` and `m === k.w` (the
   header states TS2367, measured), and an accept entry with
   `m === K.A`, `m === k.b`, `m === K.S`, and the `switch` with
   `case K.A` and `case k.b`, with output. Both fail at the pin.
2. A test that each form of the table gives the result of rule 2 or 3.
3. The full gate passes.

### 316.3 Phase Review

Before the review, a measurement found that round 1 excluded the
instance fields of a class with a constructor body, so `m === k.w`
stayed accepted there, and that `this.w = "yyy"` in a constructor was
accepted at `51135545`; `tsc` rejects both. Rule 4 is amended.

One fresh review found one MAJOR finding, realistic and older than
§316: a constructor store to a `readonly` field with a number or a
boolean literal initializer took another value (`readonly n = 0` with
`this.n = c`); `tsc` gives TS2322. Rule 4a is added. Four MINOR
findings: `readonly b = this.a` read no literal (rule 4b, fixed); the
store message printed a Rust debug value (fixed); two contrived forms
are recorded in 316.4. Over 1,126 corpus files, the CLI output differs
from the pin in the new entries only, and each is Red at the pin.

### 316.4 Open

1. A store that `tsc` accepts can be rejected with a `TscRejects`
   claim: `this.w = c ? "zzz" : "zzz"`. Contrived.
2. `switch (k.z) { case m: }` reports that the case label `"zzz"` is
   not a member; the literal is the subject. The rejection is correct
   (`tsc` TS2678). Contrived.
