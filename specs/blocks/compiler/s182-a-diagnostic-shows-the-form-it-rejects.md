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

§157.3 item 1 is fixed at HEAD (§173.1 rule 5). The current §154
total tests compare the message text and run the example fragments,
but they do not check that the TypeScript example of a site reaches
that site, or that the subscript example is accepted.

### 182.1 Rules

1. **Each listed diagnostic** takes the message, rule, reason, and
   examples that the note proposes for it, and the §175 test takes the
   proposed name. A suggested form in a diagnostic is a form that the
   CLI and `tsc` accept.
2. **Examples come from the site.** The TypeScript example of a
   rejection site is derived from that site's own §154 witness program,
   which the §154 tests already show reaches that site; a hand-written
   TypeScript example that a test does not tie to its site is removed.
   The subscript side of a site is one of two kinds, stated in the site
   table: a program, or guidance text. A gate test checks every site:
   `tsc` gives the TypeScript example the result that the site records
   (accepts for a divergence site, rejects for a site that §154 marks
   as a `tsc` rejection), the checker rejects it at that site (the
   first diagnostic), and the checker and `tsc` accept each
   program-kind subscript example. A site that no program reaches
   first, because the form it needs always meets an earlier guard (for
   example `super()` needs `extends`), is marked shadowed in the site
   table with the guarding site; the test checks that the witness's
   first diagnostic is at that guarding site and that a later
   diagnostic is at the shadowed site. A site with no witness has one, or
   the site table marks it unreachable with the reason, and the §154
   index agrees. Guidance text is not run. The test
   reads the site table, so a new site is checked with no test change.
2a. **An example fits the form the user wrote.** A site can cover
   several forms; its example entries are keyed by the selector that
   the diagnostic message matches. A diagnostic shows the entry whose
   selector its message matches, and no example when no entry matches:
   no fallback to another form's entry. A subscript program applies to
   the form of its entry's witness only; a line that names other forms
   (a shared generic example) is removed. A total gate test runs every
   §154 witness of every site: the first diagnostic's selected entry,
   if any, derives from a witness whose diagnostic matches the same
   selector.
3. Every site that rule 2 reports is fixed in this section.
4. No language rule changes: the accepted and rejected programs stay
   the same.

### 182.2 Acceptance

1. Each listed diagnostic: a test reads its new text.
2. Rule 2's test, with a firing control: a site whose TypeScript
   example is changed (in the test, not in the table) to a program
   that another site rejects makes the test fail; a program-kind
   subscript example that the checker rejects makes it fail. State its cost
   (core principle 15).
3. Goldens: the generated docs (the language reference shows the
   diagnostics) and the reject entries whose `expected-error` line
   quotes a changed message move. No `.expected` output of an accept
   entry moves.

### 182.3 Sections this one amends

§143.3 item 2, §149.3 item 2, §153.3 item 2, §157.3 item 1 (closed by
§173), §167.3 items 1 and 3, §170.3 item 3, §175.4 item 3: closed.
