# §315: local facts at loop back edges

Contract pin: `d1718851`.

## Red evidence

TypeScript 5.9.2 gives these results with the repository prelude and strict options.
The pin binary comes from a separate source archive and target directory.
The working tree never changes to the pin.

| Entry | Program | TypeScript | Pin checker | Golden |
| --- | --- | --- | --- | --- |
| `a462-loop-store-order` | a | accepts, exit 0 | S011 at 13:14 | `1`, `1`, `1` |
| `r520-loop-nullable-continue` | b | TS18047 at 14:14, exit 2 | S011 at 14:14 | none |
| `a463-loop-null-restore` | c | accepts, exit 0 | S011 at 13:14 | `1`, `1`, `1` |

The bodies match the three contract rows.
The accept entries have `js-comparable: yes`.
The development tier, ship tier, interpreter, and JavaScript checks compare their output with the goldens.

## Change

The checker assumes whole-local entry facts at a loop head.
It reads normal flow at the body end and each continue edge of that loop.
A `for` update reads the intersection of those edges.
The fact after the update must hold before control returns to the head.

If an assumed fact fails, the checker restores its state and checks the loop with the §162 facts.
Only the second check gives diagnostics.
A nested loop applies the same rule in each outer check.
The §162 rule 3b check still applies to facts without a §315 proof.

A transaction records changed map entries and vector records.
Its markers do not copy unrelated declarations.
The transaction also restores generated instances, lambda records, regex globals, and diagnostic suppression state.
The checker performs at most two body checks for §315 in each enclosing check.

The `for` initializer and the `for…of` subject establish facts before the loop transaction starts.
Field-path and shared-location rules remain under §162, §124, and §159.

## Test cost

The tests use one test thread and a debug build.
No new standing test starts an engine subprocess.

| Test | Measured time |
| --- | ---: |
| Back-edge shapes: nested block continue, while, for-of, update, inner continue | 0.014734 s |
| Nested loop facts | 0.003943 s |
| Retry diagnostics | 0.000999 s |
| For initializer facts | 0.018104 s |
| For-of subject facts | 0.003850 s |
| Retry state: generic class, lambda, regex, type-only rejection | 0.005237 s |
| Journal copy count at 1 and 1,000 records | 0.000282 s |
| Nested journal commit and rollback | 0.000025 s |

The copy-count test measures zero copies at a marker and at an append.
Two writes copy two records at both sizes.
The journal test also checks queue removal and replacement across a rollback.

TypeScript accepts the new positive edge forms.
It rejects all five corresponding nullable-edge controls with TS18047.
TypeScript also accepts the revised generic-field, inferred-local, shadowed-local, and break-only positive forms.

## Corpus check cost

The measurement uses the same accept, reject, warning, and trap programs at both revisions.
It includes the three new entries at the pin.
The timer covers `check_program` calls and result disposal.
It excludes corpus file reads, builds, engine execution, and test-process startup.
Each timing run uses `--test-threads=1`.

The existing ignored `corpus_checker_wall_time` test checks 1,044 programs.
Each revision runs five passes in one test process.
The revisions run in sequence after the other checks end.
The test uses the debug profile at both revisions.

| Revision | Five check times, sorted (seconds) | Median |
| --- | --- | ---: |
| Pin `d1718851` | 3.632703, 3.636235, 3.637083, 3.645082, 3.646849 | 3.637083 s |
| Change | 3.653931, 3.654243, 3.654506, 3.659429, 3.671033 | 3.654506 s |

The median ratio is 1.004790, an increase of 0.479%.

Measurement command:

```sh
cargo test --offline --locked -p subscript-compiler --test array_of_and_map_copy corpus_checker_wall_time -- --ignored --test-threads=1 --nocapture
```

The pin runs the same test from its separate archive and target directory.

## Verification

| Command | Result |
| --- | --- |
| `node_modules/.bin/tsc -p tsconfig.json` | PASS, exit 0 |
| `cargo test -p subscript-compiler --tests` | PASS, exit 0, full rerun |
| `cargo test -p subscript-compiler --lib` | PASS, exit 0, 683 tests |
| `cargo test -p subscript-codegen --tests` | PASS, exit 0 |
| `cargo test -p subscript-cli --tests` | PASS, exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets` | PASS, exit 0; library warnings 4/10/13 |
| `cargo fmt --check` | PASS, exit 0 |

`apparent_type_shapes` passes all eight tests.
The LIR test passes all 78 tests.
The LIR corpus snapshot has no change.
No HIR or LIR record changes.
No existing output golden changes.

The compiler tests also measure every corpus TypeScript header and each JavaScript comparison.
The codegen tests compare the development tier, ship tier, and interpreter with the corpus goldens.

## Phase review

An independent agent reviewed the cumulative diff without conversation context.
The first review found global state copies at every loop checkpoint.
The journal replaces those copies.
The second review found generic-function class maps outside the transaction.
The transaction now includes those maps and the other reachable persistent mutations.
The final review found no open realistic MAJOR or CRITICAL finding.

Scope note: §315 acceptance calls `do…while` outside the subset, but §203 already supports it.
The handoff excludes that form from its new acceptance tests.
Its existing §203 and §162 flow remains a MINOR missing-fact case from the review.

All test commands and measurement commands completed.
The independent review agent completed its work.
No background task from this round remains.
The work has no commit and no index change.

`tools/hygiene.sh` passes with exit 0.

## §315-2: finalizer effects and linear retries

The earlier verification and review sections describe §315-1.
The §315-2 contract is `11afa3d5`.
The §315-3 contract is `eb3bd8c6`; rule 1b supplies the cause form that closes the diagnostic gap.

### Rule 1a

The flow regression fails before the fix: q01 is accepted.
The revised checker holds jump edges until the finalizer check ends.
It applies finalizer effects before it returns the edges to their targets.
Nested finalizers apply their effects in run order.
An unconditional non-null store keeps an existing jump fact; it cannot restore an ended fact.
Scope exit prevents a finalizer's inner binding fact from reaching a different outer binding.

The acceptance test checks 20 programs in 0.044995 seconds.
It covers continue, loop break, switch break, catch exits, nested finalizers, scope identity, and a finalizer jump that replaces another jump.
The flow test passes.

The reject entries are `r521-loop-break-finalizer` and `r522-switch-break-finalizer`.
TypeScript 5.9.2 accepts both with the strict repository options and prelude: exit 0.
The §315-1 CLI accepts both before these fixes.
The separate pin CLI accepts `r522` and rejects `r521` with S011.
The revised checker rejects both with S011 at their final member reads.

The §315-2 corpus diagnostic check fails for both entries:
`tsc accepts but the divergence block is absent`.
This is the red evidence for §315-3.
The §315-3 form and completed result appear below.

### Rule 4 and minor findings

A body context shares a failed-assumption set across its flow snapshots.
Each later check of that loop uses the §162 facts at once.
The count test passes at depths 1, 5, and 10: 3, 7, and 12 innermost checks.
The count includes the preliminary pass and equals depth plus two.
The final isolated depth-10 check takes 0.012366 seconds with one test thread.

Global lambda defaults locate the signature key before they change that signature.
Result dependencies invalidate only cached ancestors of the changed owner.
The rollback test preserves unrelated cached results and restores the affected results.
Map retention copies only removed entries into the journal.
The lexical initializer test again includes a rejecting `let` control with a nullable store.
The compiler library suite passes all 685 tests after these changes.

### Verification of §315-2

The first library run fails the static diagnostic-code scan in the new cost test.
The test now uses the same code-string check as the existing non-null store tests.
The repeated library run passes.
The repeated compiler test command fails only at the new corpus divergence check before it can run later test binaries.

| Command | Result |
| --- | --- |
| `node_modules/.bin/tsc -p tsconfig.json` | PASS, exit 0 |
| `cargo test -p subscript-compiler --tests` | FAIL, exit 101; `r521` and `r522` lack the required divergence block |
| `cargo test -p subscript-compiler --lib` | PASS on rerun, exit 0; 685 tests |
| `cargo test -p subscript-codegen --tests` | PASS, exit 0 |
| `cargo test -p subscript-cli --tests` | PASS, exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets` | PASS, exit 0; library warnings 4/10/13 |
| `cargo fmt --check` | PASS, exit 0 |

### Repeated corpus cost

Both revisions check the same 1,046 entries, including `r521` and `r522`.
The debug test executables run in sequence after the verification commands end.
Each executable runs `corpus_checker_wall_time --ignored --test-threads=1 --nocapture`.
The timer has the same boundaries as the earlier corpus measurement.

| Revision | Five sorted check times, seconds | Median |
| --- | --- | ---: |
| Pin `d1718851` | 3.630877, 3.636811, 3.638257, 3.642730, 3.653661 | 3.638257 s |
| §315-2 candidate | 3.639499, 3.641953, 3.646969, 3.648691, 3.667746 | 3.646969 s |

The median increases by 0.2395%.
These times measure the candidate with the unresolved diagnostic form.
They do not establish completed acceptance.

All shell jobs from this round finish, including verification and corpus measurements.
This round starts no subagent, monitor, or scheduled job.
No commit or index change occurs.
The round does not run `tools/gate.sh`.
`tools/hygiene.sh` passes with exit 0.

## §315-3: finalizer causes in the flow form

Rule 1b is implemented at contract `eb3bd8c6`.
The flow carries a local path and its lexical scope with the finalizer jump cause.
Loop and switch edges carry the cause with their live facts.
The §315-3 joins retained a cause from either incoming path; §315-4 below corrects that union.
Scope exit removes a cause of that scope, so a hidden binding cannot inherit it.

Loop back edges record their causes for a failed induction.
The fallback check restores those causes before the loop head check.
An enclosing-loop retry reads the same causes without another speculative body check.
The change adds no loop body check.

A store or a null check that establishes the fact replaces the cause for its binding.
A finalizer summary preserves the cause from the checked flow instead of applying its stores to that cause twice.
Nested finalizers apply their stores in run order.
A non-null restoration on normal flow prevents the finalizer divergence at the later read.
A held jump cannot gain a fact from that restoration.

`NullableMemberFinallyJump` selects S011 with `FinallyJumpNarrowing`.
The C7 block shows the rejected finalizer jump and a null check after its target.
The checker and TypeScript accept the alternative fragment.
The example fragment reaches this site as its first diagnostic.
The §154 witness index holds `r521` and `r522`, with their measured empty TypeScript code sets.
The §312 reviewed list holds both accepted witnesses.
Both witnesses have the same cause form as the example.
The corpus test pins the complete block text for both entries.
The corpus headers still state `tsc: accepts`, TypeScript 5.9.2, exit 0.

The new flow test checks 166 programs without an engine subprocess.
It covers target joins, conditional and short-circuit joins, nested finalizers, catch joins, and enclosing finalizers.
It checks loop-head causes for `for`, `while`, and `for…of` continue edges.
It checks scope identity and the replacement of a cause by a test or store.
A later nullable store selects the ordinary nullable rejection without a divergence block.
With `--test-threads=1`, the isolated test takes 0.177863 seconds.

### Verification of §315-3

| Command | Result |
| --- | --- |
| `node_modules/.bin/tsc -p tsconfig.json` | PASS, exit 0 |
| `cargo test -p subscript-compiler --tests` | PASS, exit 0; 113.550 s |
| `cargo test -p subscript-compiler --lib` | PASS, exit 0; 685 tests; 3.132 s |
| `cargo test -p subscript-codegen --tests` | PASS, exit 0; 330.991 s |
| `cargo test -p subscript-cli --tests` | PASS, exit 0; 43.325 s |
| `cargo clippy --offline --locked --workspace --all-targets` | PASS, exit 0; 9.872 s; library warnings 4/10/13 |
| `cargo fmt --check` | PASS, exit 0; 2.186 s |

The §154, §307, §309, §311, and §312 total checks pass.
The corpus diagnostic check passes with the new blocks for `r521` and `r522`.
The codegen golden comparison passes across the development tier, ship tier, and interpreter.
Clippy reports the baseline library counts: compiler 4, runtime 10, and codegen 13.

All shell jobs from §315-3 finish.
This round starts no subagent, monitor, or scheduled job.
No commit or index change occurs.
This round does not run `tools/gate.sh`.

`tools/hygiene.sh` and `git diff --check` pass with exit 0.

## §315-4: second review fixes

The amended contract is `5d82b52e`.

A held jump keeps only facts present at the jump statement, minus finalizer effects.
A non-null finalizer store preserves an existing whole-local fact but cannot establish one on that edge.
Nested finalizers preserve the earlier cause when a later non-null store cannot restore the jump fact.

Every cause join reads the facts and causes of each incoming edge.
An edge without either the fact or the cause removes that cause from the join.
The failed-induction memo now uses the existing `LoopEdge` form, with both facts and causes.
The fallback loop head joins that edge with its entry flow.
This change adds no body check and changes no HIR or LIR form.

A null comparison alone does not erase a cause.
The branch that establishes the fact replaces its cause.
A nullable branch retains the cause through a later join.

`NullableNominalAssignmentFinallyJump` and `NullableCallFinallyJump` complete the S005 and S100 sites beside the S011 member site.
Each selects `FinallyJumpNarrowing` from the local's cause.
Each has a §307 form a alternative and a §311 example that reaches its own site first.
Each has two §154 witnesses: one loop break and one switch break.
The §312 rows list those two TypeScript-accepted witnesses for each site.
Both witnesses have the cause and use form of their site's example.

The new corpus entries are `r523` through `r529`.
Entries `r523` through `r527` require S011 without a divergence block.
Entries `r528` and `r529` require the S005 and S100 divergence blocks.
The corpus registration table and generated references include all seven entries.

### Measured probes

TypeScript 5.9.2 uses the strict repository configuration and prelude.
The eight handoff probes run in one batch from `corpus/accept/zz_probe_*.ts`.
That batch exits 2 with exactly five TS18047 diagnostics.
The other three files have no TypeScript diagnostic.
All temporary probe files are removed after the measurement.
The checker column below comes from the rebuilt CLI; every rejection exits 1.

| Probe | TypeScript | Earlier candidate checker | Revised checker |
| --- | --- | --- | --- |
| Nullable store before a loop break; non-null finalizer | TS18047 at 5:152 | accepts | S011; no block |
| Nullable store before a switch break; non-null finalizer | TS18047 at 5:151 | accepts | S011; no block |
| Nullable store before a continue; non-null finalizer | TS18047 at 5:100 | accepts | S011; no block |
| Normal nullable back edge beside a finalizer continue | TS18047 at 5:101 | S011 with a false divergence block | S011; no block |
| Normal nullable branch beside a finalizer break | TS18047 at 5:168 | S011 with a false divergence block | S011; no block |
| `take(e)` after the finalizer loop | accepts; no diagnostic | S005 without a divergence block | S005; `FinallyJumpNarrowing` block |
| Null comparison, then `e.x`, after the finalizer loop | accepts; no diagnostic | S011 without a divergence block | S011; `FinallyJumpNarrowing` block |
| Nullable function call after the finalizer loop | accepts; no diagnostic | S100 without a divergence block | S100; `FinallyJumpNarrowing` block |

Two existing catch controls also receive direct TypeScript measurements.
`try { while (true) { try { break; } finally { e = mk(); } } } catch (err) {}` rejects the later `e.x`.
TypeScript gives TS18047 at 2:157, and at 2:166 with an empty outer finalizer.
The revised tests require ordinary S011 for these controls.

A second batch checks 18 join controls and the four new site fragments.
For each nullable-use site, a normal nullable branch removes the finalizer cause through five joins.
Those joins are `if`, a conditional expression, short-circuit flow, switch exits, and catch exits.
TypeScript rejects the member controls with TS18047, argument controls with TS2345, and call controls with TS2721.
The checker gives ordinary S011, S005, and S100 respectively.
The three null-comparison controls retain the cause; TypeScript accepts each.
TypeScript also accepts both new example fragments and both null-check alternatives.

The eight-probe test takes 0.007953 seconds with one test thread.
The 18-control test takes 0.036870 seconds with one test thread.
Neither test starts an engine subprocess.
The existing finalizer-cause test takes 0.155415 seconds for 166 programs after the corrected catch expectations.

### Verification of §315-4

The first library run reports an overlong S005 example and stale generated references.
The S005 example now uses a direct null store, so it stays within the form a statement limit.
The generator refreshes the references.
The first compiler integration run also reports the missing corpus registration rows.
The rows are added, and the generator runs again because that table selects the reference examples.
Subsequent verification results appear below.

| Command | Result |
| --- | --- |
| `node_modules/.bin/tsc -p tsconfig.json` | PASS, exit 0 |
| `cargo test -p subscript-compiler --tests` | PASS on rerun, exit 0; 24 narrowing-flow tests and 50 corpus-reject tests |
| `cargo test -p subscript-compiler --lib` | PASS, exit 0; 685 tests |
| `cargo test -p subscript-codegen --tests` | PASS, exit 0 |
| `cargo test -p subscript-cli --tests` | PASS, exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets` | PASS, exit 0; baseline library warnings 4/10/13 |
| `cargo fmt --check` | PASS, exit 0 |

The first Clippy run adds one collapsible-condition warning and eight test assertion warnings.
Equivalent condition and assertion forms remove those warnings before the final Clippy run.
The final compiler library warning count is four, runtime ten, and codegen thirteen.

The §154, §307, §309, §311, and §312 total checks pass.
The ten-level failed-induction test also passes.
`tools/hygiene.sh` and `git diff --check` pass with exit 0.

The final codegen rerun and CLI rerun both finish with exit 0.
All shell tasks from this round finish.
This round starts no subagent, monitor, or scheduled job.
No temporary probe remains in the corpus.
No commit or index change occurs.
This round does not run `tools/gate.sh`.
