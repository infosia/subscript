<!-- §309 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 309. A site that only tsc-rejected programs reach claims no tsc acceptance

*(Added 2026-11-16.)* Origin: work unit 4 of the owner request of
2026-11-15 (`specs/tracking/divergence-alternatives.md`), and §307.4
item 1.

Problem, measured at `23e6f607` by the survey from the recorded `tsc`
codes of the witnesses. Nine `Diverges` sites have only witnesses that
`tsc` rejects: `Api(DateLocalGetYear)`, `Api(FormGroupBy)`,
`ContextBytesTypeCount`, `ContextBytesArgumentCount`,
`NumberMethodArgumentCount`, `ArrayFromArgumentCount`,
`NumberPredicateCount`, `JsonParseTypeCount`, and `ForOfKeys`. Each
renders "= TypeScript accepts:" for a program that `tsc` rejects.

The message of `Number(value)` says "use `value as f64`", and the
checker rejects `"1.5" as f64`. The block of the same diagnostic shows
`parseFloat`.

169 `Diverges` sites have witnesses of both kinds. Work unit 2 (an
example for each site) takes them, because it gives each site the form
that the site rejects.

### 309.1 Rules

1. Before a change, measure each of the nine sites again at the pin:
   find each program form that reaches the site, and measure `tsc` on
   each. A site that `tsc`-accepted programs do not reach becomes
   `TscRejects`, with no block. A site that a `tsc`-accepted program
   reaches keeps `Diverges`, and gets a witness of that program.
1a. A call with a spread argument goes to the spread site of its
   callee before a count site, as the iterator methods do
   (`IteratorMethodArgumentCount`). So a count site that only a spread
   with a `tsc`-accepted count reached becomes `TscRejects`, and the
   spread program gets the block of the spread site. This applies to
   `NumberMethodArgumentCount`, `ContextBytesArgumentCount`,
   `ArrayFromArgumentCount`, `NumberPredicateCount`, and
   `JsonParseCount`. *(Added 2026-11-16 after the Phase Review: the
   witness `x.toFixed(1, ...[])` kept a block that says "TypeScript
   accepts" for `x.toFixed(1, 2)`, which `tsc` rejects.)*
2. A site that becomes `TscRejects` and has a form that the checker
   accepts for the same intent names the form in a note, as §308 does.
3. The message of `Number(value)` and of `new Number(value)` names a
   form that the checker accepts for each argument type that reaches
   it: `parseFloat` for a string, `as f64` for a number type,
   `v ? 1.0 : 0.0` for a `boolean`, and `(v as i32) as f64` for an
   integer enum. *(Amended 2026-11-16 after the Phase Review.)*
4. §154 checks rule 1 totally: a `Diverges` site whose witnesses are
   all `tsc`-rejected fails the total test, as a `TscRejects` site with
   a `tsc`-accepted witness fails it now.

### 309.2 Acceptance

1. The tracking note records the measurement of rule 1 for each site.
2. Red first: the total check of rule 4 fails at the pin with the sites
   of rule 1 that change.
3. A unit test for rule 3 with a string and a number argument.
4. No program changes from accepted to rejected or back. No `.expected`
   golden moves.

### 309.3 Phase Review

Evidence: `specs/tracking/divergence-alternatives.md`. At the contract
pin, the total check of rule 4 fails with the nine sites of the
problem, and with no other site.

One fresh review found two MAJOR findings, both realistic, and eight
MINOR findings.

1. MAJOR: the first round kept four count sites as `Diverges` with a
   witness `x.toFixed(1, ...[])`. A program with plain extra arguments,
   which `tsc` rejects, still showed "TypeScript accepts" with an
   unrelated example. Rule 1a sends a call with a spread argument to
   the spread site first, so the five count sites are `TscRejects`.
2. MAJOR: `Number(v)` with a `boolean` or an integer enum named forms
   that the checker rejects. Rule 3 is amended.
3. MINOR, fixed: `new Number(value)`; the alternative column of the API
   reference; the divergence values of two API rows; two note texts;
   the header of `r47`; the negative control of rule 4.
4. MINOR: the tracking note of acceptance item 1 is Claude's work, and
   is written.

### 309.4 Open

None.
