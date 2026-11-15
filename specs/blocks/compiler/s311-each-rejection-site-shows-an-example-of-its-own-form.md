<!-- §311 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 311. Each rejection site shows an example of its own form

*(Added 2026-11-16.)* Origin: work unit 2 of the owner request of
2026-11-15 (`specs/tracking/divergence-alternatives.md`), and §182.4
items 1 and 2.

Problem, measured at `23e6f607` by the survey. A `Diverges` site shows
the example of its `Divergence` variant (§79), or one of 13 fixed
examples (§182). At 184 sites (an upper bound), the `ts` fragment of
the example, checked alone, does not reach the site that emitted the
diagnostic: the example shows another form. Example: `take(...xs)`
shows a `splice`/`unshift` example. §182.4 item 1 found that the
message does not identify the form; each `RejectionSite` now does
(§153, §154).

### 311.1 Rules

1. Each `Diverges` site has one example: the `DivergenceEntry` of its
   variant, or an entry of its own. A site takes an entry of its own
   when the `ts` fragment of the variant entry does not reach the site.
   The entry of a site holds `ts` and `subscript`, and a `why` of its
   own where the `why` of the variant is not true for the form of the
   site. The record id is that of the variant. *(Amended 2026-11-16
   after the Phase Review: the 13 fixed examples of §182 had a `why` of
   their own, and the `why` of the variant was false for some.)*
2. The `ts` fragment of the example of a site, checked alone, reaches
   that site. Its `subscript` fragment is form a or form b of §307
   rule 1, and form a is accepted by the checker and by `tsc`. The `ts`
   fragment is accepted by `tsc`.
3. The example of a site is selected by the site, not by the message.
   The 13 fixed examples of §182 become entries of their sites.
4. A total check closes rules 1 and 2: for each `Diverges` site, it
   checks the `ts` fragment of the example that the site shows, and
   fails if the fragment does not reach the site. It reports every
   failing site at once. A site with no `tsc`-accepted program has no
   example and is `TscRejects` (§309 rule 4).
5. An entry of a site is short: one to three lines, the shortest form
   that reaches the site. No witness text is embedded.
6. The block layout of §79 rule 3 does not change. The record id and
   its spelling (`collisions.md C24`) do not change.
7. A site that two forms of a user reach, where one example does not
   fit both, splits into a site for each form, so that each form shows
   its own example. The Phase Review measured three: `keyof T` at
   `OperatorAnnotation`, `Object.keys(x)` at `UnknownAmbientValueName`,
   and `arr.entries()` in a `for…of` head at `ForOfEntries`. *(Added
   2026-11-16 after the Phase Review.)*
8. The example of a site is on the first diagnostic of its `ts`
   fragment.

### 311.2 Acceptance

1. Red first: the total check of rule 4 fails at the pin. The tracking
   note records the count and the list of the failing sites.
2. After the change, the check passes. The tracking note lists each
   site that takes an entry of its own.
3. The cost of the total check is measured with `--test-threads=1` and
   stated in the test. The CLI time of a rejected file and of an
   accepted file does not change by more than the noise of the debug
   CLI (§182.4 item 1 measured 18.6 ms against 3.9 ms for a prototype
   that embedded witness text).
4. No message line or position moves. No program changes from accepted
   to rejected or back.

### 311.3 Phase Review

### 311.4 Open

1. The total check of rule 4 reads only the example fragment. It cannot
   find a form that reaches the same site and that the example does not
   fit (rule 7). 114 `Diverges` sites have more than one witness file;
   the Phase Review sampled them and found the three of rule 7. No
   mechanical check reads whether two forms are the same form.
