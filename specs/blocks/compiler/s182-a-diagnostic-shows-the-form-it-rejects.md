<!-- §182 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 182. A diagnostic shows the form it rejects

*(Added 2026-10-09.)* Origin: the message defects in the Open lists of
§143, §149, §153, §157, §167, §170, and §175, and two that the async
review of 2026-10-08 found (S013 for `.then`, and S009 after §181).
The measurement round at `93453f87` is
`specs/tracking/s182-message-measurement.md`.

Problem: a diagnostic carries a message, a rule, a TypeScript example,
a subscript example, and a reason. The measurement found eight
diagnostics where one of these parts is wrong (the note gives the
program, the current text, and the evidence for each):

| Note item | Site | Defect |
|---|---|---|
| 2 | a TaskGroup in a generator body (S009) | the rule and the reason name captures, not the generator scope exit |
| 3 | a handle in an async expression body; the S009 async capture; `await` of a synchronous call | a second, cascaded S100; "an async arrow captures nothing" is false after §181; the reason does not say what `await` needs |
| 4 | a generic function used as a value | the suggested form is rejected |
| 5 | a `Date` method used as a value | the example and the cited section are of another form |
| 6 | `map` that produces an array of functions | the message prints the element type wrong |
| 8 | S013 for `.then` and `.catch` | the TypeScript example is `Promise.resolve`, and no example shows the `await` form of `.then` or the `try`/`catch` form of `.catch` |
| 9 | S009 for `this` in an async arrow; S014 for `map(async …)` | the message says an async arrow captures nothing; the examples show a mutable number, not a receiver, and no handle-array form |
| 7 | a test name in §175 | "owns" names the opposite of what the test checks |

§157.3 item 1 is fixed at HEAD (§173.1 rule 5).

### 182.1 Rules

1. **Each listed diagnostic** takes the message, rule, reason, and
   examples that the note proposes for it, and the §175 test takes the
   proposed name. A suggested form in a diagnostic is a form that the
   CLI and `tsc` accept.
2. Examples stay keyed as at `93453f87`: one example set for each
   site or divergence entry. Item 1 of 182.4 records why this section
   does not key them by form.
3. No language rule changes: the accepted and rejected programs stay
   the same.

### 182.2 Acceptance

1. Each listed diagnostic: a test reads its new text, and the CLI and
   `tsc` accept each suggested form in it.
2. Goldens: the generated docs (the language reference shows the
   diagnostics) and the reject entries whose `expected-error` line
   quotes a changed message move. No `.expected` output of an accept
   entry moves.

### 182.3 Sections this one amends

§143.3 item 2, §149.3 item 2, §153.3 item 2, §157.3 item 1 (closed by
§173), §167.3 items 1 and 3, §170.3 item 3, §175.4 item 3: closed.

### 182.4 Open

1. **An example does not identify the form the user wrote.** One site
   covers several forms, and its examples fit one of them. A prototype
   at `80902350` (reverted) derived each TypeScript example from the
   site's §154 witness and selected the example by the diagnostic
   message. Its first total check found 543 sites where an example
   does not fit: 248 TypeScript examples reach another site
   first, and 340 subscript examples are guidance text, not programs.
   The prototype brought the total check to 0 sites, but its
   verification review found that the message does not identify the
   form: 207 groups of witnesses share one message (`x **= 3`,
   `export *`, `extends`, `keyof`, `entries()`), and a message that
   quotes a user name matches no selector, so it shows no example.
   The prototype also embedded 587,595 bytes of witness text; its debug
   CLI median was 18.6 ms for a rejected file and 3.9 ms for an
   accepted file.
   A fix needs a form identity that each checker rejection site
   carries; the message is not one.

2. **Sibling forms of two listed sites keep the site example.**
   `[1].flatMap((v: i32) => [work()])` reports that `flatMap` cannot
   carry a counted callback result and shows the `toFixed` example. A
   `map` callback that returns a nullable function prints the correct
   element type, but shows the same example: the function-array entry
   matches only a non-nullable function type. Item 1 is the class.
3. **`.then`, `.catch`, and `.finally` are rejected on any receiver.**
   `class C { then(): void {} } new C().then();` reports "Promise
   combinator `.then(...)`" with the handle example. The receiver test
   is missing at `93453f87` too.
