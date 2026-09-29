# §129 host API module: acceptance measurement

Status: the third-review fixes and targeted checks are complete. The final full-gate verdict is recorded separately.
Initial measurement baseline: `9815325`. Implementation contract: `5abd2b3`.

## Method

A temporary checker prototype read the resolved entry export graph after body checks.
The harness supplied the entry source name explicitly for each corpus program.
It used the corpus loader entry, independent of the checker file order.
The prototype checked export kinds and boundary signatures under §129 rules 2–7.
It compared the old host names with the valid entry export names.
Generic declarations failed before monomorphization hid their exports.
The prototype replaced the program-wide host-name check for this measurement.
It did not implement the public program-input API or migrate downstream consumers.

The harness checked each accept entry at baseline and with the prototype.
All 280 baseline entries passed. Two entries failed with the prototype.
All other accept entries passed the prototype check.
The checker runs two passes; repeated symbol reports were deduplicated below.

Command:

```text
cargo test --offline --locked -p subscript-compiler --test s129_measure -- --nocapture
```

The temporary measurement test passed. Its success did not establish §129 acceptance.
The test took 2.49 seconds. The incremental build took 1.68 seconds.

## Acceptance changes

| Entry | Baseline | Prototype diagnostic |
| --- | --- | --- |
| `a143-async-generic` | accepted | S100 at `a143-async-generic.ts:35:23`; generic function `tick` cannot be a host entry. |
| `a281-function-names` | accepted | S100 at `main.ts:8:17`; host entry `read` must return void. |

The first entry exports a generic async function from its sole program module.
The second entry exports an `i32` function from `main.ts`.
Both violate §129 rule 4.

Exact prototype diagnostics:

```text
a143-async-generic.ts:35:23: entry export `tick`: generic functions cannot be host entries [S100]
main.ts:8:17: entry export `read`: host entries must return void; target `read` in `main.ts` [S100]
```

## Host-name changes

| Entry | Baseline host names | Prototype host names |
| --- | --- | --- |
| `a287-renamed-imports` | `bump`, `main` | `main` |
| `a288-re-export-kinds` | `bump`, `main` | `main` |

These sets came from checker data. Emitted symbols were not measured.
No other accept entry changed its candidate host-name set.

## Stop condition and remaining work

The handoff requires a stop when an accepted program becomes rejected.
The two measured rejections activate that condition.
All prototype production edits and the temporary test were removed.
No Rust file remains changed. No file split was needed.
No golden changed. No git write command ran.

The full acceptance 1 measurement is incomplete:
reject entries, examples, and execution under the new rule were not measured.
No new corpus entries were added, so no new-entry Red or `tsc` measurement ran.
No downstream implementation, generated-document update, or final build ran.
The full gate did not start; there is no gate verdict.

The owner resolved these accept-entry conflicts at `d79d7e2`.

## Resumed scope check at `d79d7e2`

The owner keeps rule 4 and authorizes two source rewrites.
`a143-async-generic` becomes a directory entry, with `tick<T>` in a sibling module.
`a281-function-names/main.ts` drops `export` from `read`.
These rewrites are authorized but not applied in this resumed check.

A separate scope conflict prevents implementation under the current handoff.
`corpus/reject/r267-duplicate-host-entry` requires the program-wide host-name rejection that §129 removes.
Its entry exports `update` and `main`; its library exports another `update`.
Under §129, the library export is not a host entry, so this program must pass.

Measured at the unchanged HEAD:

```text
cargo build --offline --locked -p subscript-cli
Finished dev profile in 1.73s; exit 0, no warnings.

target/debug/subscript check corpus/reject/r267-duplicate-host-entry/main.ts
error[S017]: duplicate host entry `update` in modules `main.ts` and `lib.ts`
 --> lib.ts:8:17
exit 1

node_modules/.bin/tsc --strict --noEmit --target es2022 --lib es2022 --module esnext --moduleResolution bundler prelude/lang.d.ts corpus/reject/r267-duplicate-host-entry/main.ts corpus/reject/r267-duplicate-host-entry/lib.ts
TypeScript 5.9.2; exit 0; no diagnostics.
```

The first TypeScript probe omitted `--lib es2022` and failed on the prelude's `Worker` conflict with DOM declarations.
The command above uses the project's ES2022 library surface.

`compiler/tests/corpus_reject.rs:359` requires S017 at `lib.ts:8`.
`reject_table_covers_every_corpus_entry` also requires a test row for every reject entry.
Removing only the row cannot retire this entry.

The handoff permits new reject entries from `r277`, plus header changes on existing entries when a harness needs a new field.
It does not permit deletion, migration, or source changes for `r267`.
Its scope rule requires a stop when another file needs changes.
The next handoff must authorize retirement of `r267`, or its conversion to an accept entry under §129.

Only this tracking file changed in the resumed round.
No production changes, corpus rewrites, file splits, or git writes ran.
Acceptance 1 remains incomplete; new-entry Red tests and implementation tests did not run.
The full gate did not start, so there is no gate verdict.

## Resumed round at `5abd2b3`

Acceptance 1a authorizes the retirement of r267 and its accept replacement.
The baseline CLI binary was preserved before implementation.
The pure split moves the lib.rs tests into tests/mod.rs, tests/language.rs, and tests/collections.rs.
No signatures changed. `cross_file_class_names_have_distinct_identities` adds `SourceFile::entry("api.ts", "")`. Two other moved bodies change `SourceFile::new` to `SourceFile::entry`: `the_checked_module_carries_the_bytes_of_every_source` and `two_file_program_with_import_checks_clean`. The remaining moved bodies are unchanged. The moves and these edits occurred in the same implementation tree.

The second pure split moves lower/mod.rs host adapter functions into lower/host_entries.rs.
The checker measurement took 1.50 seconds after a 2.88-second incremental build.
All 280 accept entries passed after the authorized rewrites.
Of 259 reject entries, only r267 changed to acceptance.
Only a287 and a288 lost the host name `bump`.
The existing a143 golden keeps its bytes.
The retired name is `retired:r267-duplicate-host-entry`; a293 contains the same program.
The retirement sweep now rejects the obsolete duplicate-host-entry reason.

## Completed implementation at `5abd2b3`

The checker resolves the entry module from `SourceFile.entry`.
`SourceFile::entry` names it explicitly. The CLI and the discovery loader name their input root.
One non-ambient source remains its own entry. Ambiguous input and an ambient entry produce diagnostics.

The checker builds `hir::Module.host_entries` from the resolved export graph.
Each row carries the public name, declaration symbol, checked boundary signature, and export position.
LIR resolves the symbol to a function id and retains the signature and position.
Dev lookup, C adapters, program headers, roots, and reload read this table.
The function trap-site method supplies validation sites only; it does not select entries.
The program-wide duplicate-host-name rule and its divergence text are removed.

The ship emitter writes a program-specific header, exposed through `CProgram.host_header` and `EmittedCFiles.host_header`.
The runtime header generator supplies declarations without a fixed program `main` declaration.
Its existing standard-header output remains byte-identical.
A parameterized `main` remains callable through the host API; the standard runner rejects it.

Reload hashes public names and boundary signatures, and resolves current alias targets.
Async adapter slots depend on declaration signatures, so alias target changes cannot shift retained function slots.
The test changes an async alias target and then calls a retained function.

Existing module tests now name their entry or supply an explicit empty host API for library-only fixtures.
Tests of declaration identity keep non-boundary helper functions private in their entry module.

### Final acceptance measurement

The initial resumed checker sweep checked 280 accepts and 259 rejects in 1.50 seconds.
All rewritten accepts passed. Only r267 changed from rejection to acceptance.
The complete sweep checked 283 accepts, 261 rejects, five warning entries, and 71 trap entries.
It also checked all 17 example sources. The complete checker sweep took 2.38 seconds.
A first trap sweep omitted the interop mirror for t72; supplying the fixture mirror corrected the measurement reader.
The temporary measurement test was removed after the measurements.

| Existing entry | Host names before | Host names after |
| --- | --- | --- |
| a287-renamed-imports | bump, main | main |
| a288-re-export-kinds | bump, main | main |

No other existing accepted entry or example changed its host names.
No existing accepted entry became rejected after the authorized rewrites.
No existing `.expected` file changed.

The dev-JIT/ship-C/golden sweep compared all 283 entries, skipped none, and passed in 118.42 seconds.
The interpreter corpus checks passed in the LIR suite.
The example differential and C-host tests passed in 23.22 seconds; the Rust-host test passed in 0.05 seconds.
The hot-reload example ran on dev and ship, then retained its state across a body reload.
The same test rejected a declaration change.

The a143 directory conversion changes the source-position records in the LIR text snapshot.
Only the a143 snapshot section changed. Normalizing source positions makes the complete old and new snapshots identical.
The snapshot was regenerated through `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`.

### Corpus and Red evidence

| Entry | Baseline `5abd2b3` result | Implemented result |
| --- | --- | --- |
| a291-host-api-aliases | S017: duplicate host entry `update`, audio.ts:6:17 | Accepts; host names audioUpdate, main, physicsUpdate |
| a292-host-shared-target | Emits main and update adapters; first and second adapters are absent | Accepts; host names first, main, second; aliases share state |
| a293-module-only-update | S017: duplicate host entry `update`, lib.ts:6:17 | Accepts the former r267 program |
| r277-entry-nonfunction | Check exits 0 | S100 at main.ts:8; entry exports functions only |
| r278-entry-signature | Check exits 0 | S100 at main.ts:8; read must return void |
| r279-entry-signature-chain | Check exits 0 | S100 at main.ts:8; message names target read in lib.ts |

The Red commands used the CLI binary preserved before any production edit.
The a292 Red emission used `emit`, then read the emitted adapter declarations.
The existing r272 entry retains the duplicate-export rejection of §128 and §129 rule 3.
That rejection already exists at HEAD; it is a namespace control, not a new Red claim.
The retired r267 source program is silent, so its new a293 golden is empty.
The golden shape test accepts empty output and still requires a final newline for nonempty output.

### TypeScript and Node

TypeScript 5.9.2 used the prelude with `--strict --noEmit --target es2022 --lib es2022 --module esnext --moduleResolution bundler`.

| Entry | TypeScript result | Seconds |
| --- | --- | --- |
| a143 directory rewrite | accepts | 0.184 |
| a291 | accepts | 0.179 |
| a292 | accepts | 0.179 |
| a293 | accepts | 0.177 |
| r277 | accepts; C18 divergence | 0.174 |
| r278 | accepts; C18 divergence | 0.174 |
| r279 | accepts; C18 divergence | 0.177 |

The total TypeScript header suite passed: ten tests in 1.18 seconds.
Node v24.18.0 matched all 137 comparable entries; 146 entries declare a collision-based exclusion.
The Node comparison took 0.357 seconds.

### Tests and costs

Every new test has a positive or negative control in the same test.
The checker tests cover explicit and implicit entry selection, file reordering, ambiguous inputs, ambient inputs, and each invalid declaration kind.
They also check each public host-table field and the HIR-to-LIR entry constructor.

| Check | Result | Test seconds |
| --- | --- | --- |
| Compiler library | 336 passed | 0.08 |
| Reject corpus | 38 passed | 0.19 |
| Checker host table | 2 passed | 0.01 |
| Codegen library | 232 passed | 8.07 |
| C emitter and trap parity | 105 passed | 66.28 |
| LIR and interpreter | 53 passed | 8.57 |
| Reload corpus and compatibility | 34 passed | 3.44 |
| New dev/header/reload/example controls | 6 passed | 0.55 |
| CLI missing-main and native alias host | 1 passed | 1.66 |
| Runtime header generator | 12 passed | 0.12 |
| Module identities on all engines | 7 passed | 1.67 |
| Re-export file-order controls | 2 passed | 0.08 |

The native alias test compiles one C host against the generated program header.
Its two host calls prove that both adapters reach one implementation and one state.
The full corpus execution sweeps establish unchanged output; the small controls isolate host API behavior.

The pinned formatter passes. The offline locked workspace build, including all targets, is warning-free (21.80 seconds).
Generated documents were regenerated with the designated compiler command.
Clippy reports compiler/runtime/codegen counts of 3/18/13, below or equal to the gate baselines 7/18/13.
The clippy run took 7.49 seconds. No git write command ran.

### Final alias adapter check

Each additional dev alias now has a distinct `subscript_export_<name>` adapter.
The adapter calls the shared implementation and adds no reload slot.
A direct symbol test covers synchronous and asynchronous aliases, with a private-target exclusion control.
The three slot tests pass in 0.09 seconds.
The six host API tests pass in 0.75 seconds; the 34 reload tests pass in 3.47 seconds.
The final warning-free all-target build took 11.88 seconds. The final clippy run took 4.99 seconds.

### Known owner record and final gate

The collision-index test still reports the unmodified C14 reference to r267 at collisions.md line 450.
The handoff assigns its change to `retired:r267-duplicate-host-entry` to the owner in the implementation commit.
It explicitly says not to stop on that text. No specification file other than this tracking file was edited.
The same owner commit adds the C18 corpus references.

The one full gate starts after this record and all other work finish.
Its generated `target/gate` record and the final response carry the verdict.
No edit or concurrent work runs during that gate. A failed gate is not rerun.

## Follow-up after the first full gate

The first full gate failed four tests in both debug and release.
Debug took 491 seconds; release took 585 seconds.
The owner corrected C14, C18, and the S017 references. This follow-up leaves `specs/blocks` unchanged.

The CLI cycle fixture now exports `root(): void` and calls it through the sibling module.
The test checks successful loading and output, then rejects the old non-void entry with exactly one diagnostic.
The native alias host now passes its C body directly to `host_entry` before compilation.
The existing host-body test retains its helper-bypass firing controls.

Named TypeScript fences form one program. `main.ts` is the explicit entry, independent of fence order.
The tutorial changes only two fence info strings: `ts file=math.ts` and `ts file=main.ts`.
The harness rejects missing entries and duplicate file names.
Controls reject a library type error, a missing import, and the library incorrectly selected as the entry.
The document sweep also injects an error into a real example's library file.

Targeted results: CLI commands 22 passed in 4.00 seconds; document tests 7 passed in 0.64 seconds.
Host-body tests: 5 passed in 0.44 seconds.

### Exact LIR snapshot position change

In `codegen/tests/lir-goldens/corpus.txt`, the a143 `Vec2` declaration line changed as follows.
Old line:

```text
class c1 "Vec2" value=true descriptor=false boundary=false align=None @ a143-async-generic.ts:7:7
```

New line:

```text
class c1 "Vec2" value=true descriptor=false boundary=false align=None @ main.ts:9:7
```

This is one exact line pair from the directory conversion, not the complete snapshot diff.
The a143 section contains 95 replaced lines with source-position changes. No instruction or output value changed.
The existing `.expected` files remain unchanged. This follow-up does not edit the snapshot.

The owner authorized one new full gate after these fixes and the pre-gate checks.
No edits or other checks run during that gate. Its verdict belongs to the generated gate record and final report.

Follow-up pre-gate checks: pinned formatting passes; the all-target build is warning-free (9.86 seconds).
The designated generator refreshed the documents. Clippy counts remain 3/18/13 (3.62 seconds).
The corrected collision-index test passes. The prior owner-reference failure no longer applies.

## Phase Review fixes (s129b)

The prior authorized follow-up gate passed: debug 1944/0/3, release 1941/0/3, exit 0.
Its debug and release steps took 467 and 552 seconds. The earlier pending status was stale.

1. Dev execution now calls the generated async runner, which reads the shared LIR `async_roots` list.
   Ship C and the interpreter already read that list. The reload corpus harness now reads it too.
   The list comes from checked host entries, sorted by target id, deduplicated, and excludes the main target.
2. Capture discovers directory dependencies from `main.ts` through `discover_module_sources`.
   A subprocess test captures a291 and rejects a missing corpus id.
3. A dev run without a zero-argument host main returns S100 at the entry source, not an internal lowering error.
   CLI and direct-dev tests verify the diagnostic and a working main control.
4. The checker derives boundary parameter eligibility from `host_entry_trap_sites`; the duplicate predicate is removed.
   This checks the boundary signature, not membership in the host table.
5. Reload changes an alias from `other` to `parameterized`, with identical library declarations on both sides.
   Only the host signature changes. The test rejects the reload and verifies the old implementation still runs.
6. The r277–r279 reject rows now follow r276. Regeneration restores the earlier general S100 example.
7. The runtime header declares runtime APIs and the universal initializer, without a program-specific main signature.
   The generated program header owns its checked entry declarations and names subscript codegen in its banner.
   Standard runner/test support declares its fixed zero-argument main convention explicitly.
   A C test includes both headers in either order and accepts a parameterized main call; a wrong-arity call fails.
8. Dev lowering computes one map from target id to its first host row. All membership and alias lookups reuse it.
   The leftover bare block was removed.
9. Existing r272 supplies acceptance 3's duplicate entry-export rejection (S017); it is reused, not a new Red claim.

Before changes added lines to `codegen/src/ship.rs`, its test module moved unchanged to `codegen/src/ship_tests.rs`.
The production file now has 1,077 lines; the test file has 1,200. The compiler test split remains untouched.
No specification block or tutorial prose was edited in this round.

### New corpus and Red evidence

| Entry | Expected output | Evidence before the fix |
| --- | --- | --- |
| a294 async declaration order | `main, zeta, alpha` | The old row-based reload runner printed `main, alpha, zeta`. |
| a295 two async aliases | `main, work` | The old row-based reload runner printed `main, work, work`. |
| a296 async main alias | `main` | The old row-based reload runner printed `main, main`. |

The reload corpus sweep measured these three failures before its runner changed (3.91 seconds).
The reviewer reported the same dev-runner defects. The new JIT runner uses the same root list as ship and interpreter.
The preserved pre-implementation binary prints only `main` for a295: its local aliases are not host roots.
That binary prints the expected output for a294 and a296; these two guard regressions introduced during §129 implementation.
All three new entries run on JIT, ship C, and interpreter against hand-written goldens.
An added async export is a firing control: it adds exactly one output line.
Existing `.expected` files remain unchanged.

TypeScript 5.9.2 accepts all three with the prelude and strict ES2022/bundler options.
Measured seconds: a294 0.186, a295 0.182, a296 0.217.
Each entry declares the measured C8 runner divergence for Node.

### Targeted checks

| Suite | Result | Seconds |
| --- | --- | --- |
| Codegen library | 233 passed | 8.15 |
| Host entries, capture, and dual headers | 9 passed | 1.94 |
| Documentation | 7 passed | 0.63 |
| Test host rules | 5 passed | 0.45 |
| Reload corpus | 34 passed | 4.18 |
| CLI commands | 22 passed | 4.00 |
| CLI watch | 9 passed | 0.88 |
| Runtime header | 12 passed | 0.13 |
| Reject corpus | 38 passed | 0.18 |
| Compiler host entries | 2 passed | 0.01 |
| Node and collision index | 10 passed | 0.39 |
| TypeScript corpus | 10 passed | 1.28 |

The one authorized full gate follows formatting, the warning-free build, document regeneration, and clippy checks.
Its generated record and the Japanese report carry the final verdict. No edits or concurrent checks run during the gate.

Final pre-gate checks pass: pinned formatting; warning-free all-target build (12.58 seconds); regenerated runtime header and documents.
Clippy counts remain 3/18/13 (5.44 seconds). Existing output goldens are unchanged.

## Snapshot and benchmark-header follow-up

The Phase Review gate failed: debug 1946/1/3 and release 1942/2/3, exit 1.
The debug step took 509 seconds; release took 599 seconds.
The two causes were missing new LIR snapshot sections and the benchmark's dependency on the removed runtime main declaration.

### Generated snapshot additions

The existing generator ran with `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1` through
`coroutine_and_measurement_lir_text_matches_goldens`. Its output replaced `codegen/tests/lir-goldens/corpus.txt`.
The generated file contains exactly one insertion of 733 lines. Added ranges are inclusive:

| New section | First line | Last line | Added lines |
| --- | --- | --- | --- |
| a294-async-host-order | 21185 | 21435 | 251 |
| a295-async-host-aliases | 21436 | 21679 | 244 |
| a296-async-main-alias | 21680 | 21917 | 238 |

Removing those three generated sections reproduces the complete pre-update snapshot byte for byte.
No existing line changed in this follow-up. The previously recorded a143 position changes remain unchanged.
The generator took 1.95 seconds; the normal snapshot comparison then passed in 1.94 seconds.
No `.expected` file changed.

### Benchmark header ownership

Only `benchmarks/src/bin/perf-gate.rs` changed under `benchmarks`.
The benchmark writes the compiler's `host_header` beside its emitted C and includes `program.h` from the timing host.
It no longer obtains a program-specific main declaration from the runtime header.
Workloads, timing spans, compiler flags, and thresholds are unchanged.
The release perf-gate binary build passed in 1.14 seconds.

The authorized full gate runs once after the targeted release check and pre-gate checks.
Its generated record and final Japanese report carry the verdict. No edits run during that gate.

The targeted release benchmark test passed both tests in 7.63 seconds, including generated C compilation and all existing thresholds.
Pinned formatting passes. The workspace all-target build is warning-free (9.48 seconds); generated documents were refreshed.
Clippy counts remain 3/18/13 (3.51 seconds).


## Second Phase Review fixes (s129c)

The snapshot/header follow-up gate passed. Its record is
`target/gate/20260929T084255Z-full.md`; debug took 466 seconds and release 569 seconds.

```text
gate full e5b31029a1b3257c4874a5abd5a5dd4717cb654b dirty:97 debug 1947/0/3 release 1944/0/3 skips 2/0 clippy 3/18/13 goldens-moved 1 exit 0
```

This round follows the amended rules 10 and 10a at `88846f5`.

1. All three benchmark C hosts include the generated `program.h`. Async-cost and
   cross-language now write that header alongside their generated C. Each benchmark
   has a test compiling its actual host constant; deleting the main declaration is
   the negative control. Release async-cost with `--warmup 1 --timed 1` succeeds,
   with stable output and zero unfinished tasks.
2. Both example C hosts and the tutorial use the program header, without handwritten
   entry declarations. The permitted prose identifies its ownership and the three
   emitted files. The owner explicitly approved adding `-Igen` to the tutorial shell
   build lines. The Step 2 test extracts the real tutorial fences and compiles the
   host against the emitted header; renaming its main declaration makes compilation
   fail. The test body uses `host_entry` and removes its fixed runner prototype so
   the program header remains the declaration owner. Both example hosts build and run.
3. LIR async roots use first entry export-site source order, deduplicate targets,
   and exclude the runner-main target. All tiers consume that list. The validator
   and facts oracle check first-site order and membership; they no longer require
   target-id order. Six input-file permutations produce the same roots and output
   on JIT, ship C, and interpreter. Reversing roots fails validation; changing export
   sites changes execution order as a firing control.
4. `hir::Module::runner_main` is the checker-owned runner query. JIT, default build,
   emitted runner files, and reload/watch consult it. Missing main yields S100 at
   the stable entry position. A CLI test compares identical diagnostics for run,
   run --watch, build, and build --run, then successfully runs a valid main control.
   Named host calls still work without a runner main.
5. The compiler test-move account above now names the three changed bodies exactly.
   This round preserves them without further edits.
6. Runtime header rendering has one API, `render`, and one test module. Generated
   runtime-header bytes remain unchanged by this consolidation.
7. `SourceFile` is now non-exhaustive; its constructors cover caller use.
8. The corpus reader requires exactly `main.ts`. A directory control includes
   `a-main.ts`; a directory missing exact main is rejected.
9. Ambiguous-entry diagnostics choose the lexical-first filename, independent of
   input order. A direct test compares complete reordered diagnostics.
10. Generic re-export diagnostics name both the target declaration and its file.
    The direct checker test verifies the library filename.
11. This record includes the previous passing gate, these fixes, and the new snapshot range.

### a297 Red, TypeScript, and generated snapshot

`a297-async-export-site-order` re-exports async targets from two libraries. Its
handwritten output is `main, zeta, beta, alpha`. The preserved pre-fix CLI binary
instead prints `main, zeta, alpha, beta` (exit 0): a measured ordering failure.
The entry includes two aliases of zeta and an alias of main, so neither duplicates
execution.
TypeScript 5.9.2 accepts it with strict ES2022/bundler options and the prelude
(0.188 seconds). Its header declares the C8 runner divergence.

The designated LIR generator ran through
`SUBSCRIPT_CAPTURE_LIR_GOLDENS=1 ... coroutine_and_measurement_lir_text_matches_goldens`
(1.94 seconds). The generated snapshot adds only a297 at **21918–22176**, inclusive:
259 added lines. A complete sequence comparison against the saved pre-round snapshot
found one insertion and no replacements or deletions. All existing lines, including
the previously recorded a143 source-position changes and a294–a296 sections, remain
byte-identical. No existing `.expected` file changed.

### Targeted checks and costs

| Check | Result | Seconds |
| --- | --- | --- |
| Checker host entries and runner query | 3 passed | 0.01 |
| Codegen library | 233 passed | 8.31 |
| Host entries and reordered roots | 11 passed | 3.92 |
| Tutorial/document harness | 8 passed | 0.64 |
| Required test host helper | 5 passed | 0.44 |
| LIR and interpreter | 53 passed | 9.09 |
| Reload | 34 passed | 4.11 |
| CLI commands | 23 passed | 4.17 |
| CLI watch | 9 passed | 0.88 |
| Runtime header | 12 passed | 0.12 |
| Reject table and corpus | 38 passed | 0.18 |
| Node/collision corpus | 10 passed | 0.39 |
| TypeScript corpus | 10 passed | 1.27 |
| Three benchmark host compile tests | 1 passed per binary | 0.16 each |
| Both example C hosts | 2 passed | 25.40 |

The one authorized full gate follows formatting, a warning-free all-target build,
document regeneration, and clippy. No edits or concurrent checks run during it.
Its generated record and final Japanese report carry the verdict. No specification
block was edited by this round, and no git write command ran.

Final pre-gate checks: pinned formatting passes; the all-target build is warning-free
(11.07 seconds); clippy library counts remain 3/18/13 (7.19 seconds).
The document generator initially found missing corpus headers in a297's two library
files. Both headers were added, documents regenerated successfully, and a297's LIR
section regenerated again. The same single 259-line insertion was verified against
the pre-round snapshot; normal snapshot comparison passes (1.94 seconds).

## Accept corpus runner-query follow-up

The second-review gate failed: debug 1952/3/3 (503 seconds), release
1949/3/3 (603 seconds), exit 1. One accept-corpus test still searched
for a declaration named main, missing a297's `root as main`. The two
CLI gate failures accompanied a hygiene violation in this record; the
owner removed the local path before this follow-up.

The directory accept sweep now calls `Module::runner_main` and verifies
that its host-entry target resolves to a HIR function symbol. It accepts
a297 without requiring the implementation declaration to be named main.
The same test has a firing control: a library declaration named main
is exposed first as main (runner accepted), then as start (runner absent).
The declaration itself remains exported in both cases, isolating entry
export membership from declaration-name lookup.

All 13 accept-corpus tests pass (1.34 seconds). Pinned formatting passes;
the offline locked workspace all-target build is warning-free (0.59 seconds).
The designated generator refreshed the documents. No production code,
corpus source, or snapshot changed in this follow-up. The authorized full
gate runs once after these checks; its generated record and final report
carry the verdict. No edits or concurrent checks run during the gate.


## Third Phase Review: scope stop

The previous gate passed: debug 1955/0/3, release 1952/0/3, exit 0.
Its record is `target/gate/20260929T095223Z-full.md`.

The third review follows `2647d37`. The partial implementation makes program C
include its header and gives the test-host helper a required program-header argument.
AOT file emission substitutes the emitted header label. The named hand declarations
and the tutorial test's prototype-removal workaround are removed.
Reload now retains the generated async runner for each generation.
Watch calls main, kicks that runner, and pumps pending jobs.
An initial missing-main diagnostic now enters the normal watch diagnostic state.
The new CLI tests cover both corpus programs and recovery after a missing main.
Their compilation currently fails with E0282/E0283 at an untyped closure result.
Those tests have not run.

Header controls use explicit implicit-declaration errors on GNU-style and MSVC compilers.
Named-entry diagnostics now select a named file, independent of input order.
The example hosts' extra blank lines are removed.

The total declaration check found three declarations outside the allowed edit scope:

- `corpus/interop/interop.c:192`: `subscript_export_adopt`.
- `corpus/interop/wire-enum.c:16`: `subscript_export_configure`.
- `corpus/interop/wire-enum.c:25`: `subscript_export_configure`.

Both files also define weak fallbacks of those symbols.
The handoff excludes `corpus/interop/**` and requires a stop if another file is needed.
No exemption was added to the total check. Neither excluded source was edited.
The full gate did not start, and this round has no verdict.
The final build, release benchmark build, generators, and hygiene remain pending.

Targeted results before the stop: checker host entries 3 passed (0.01 seconds);
codegen library 233 passed (8.07 seconds); async clearance 1 passed (2.23 seconds);
documentation 8 passed (0.64 seconds); host entries 12 passed (4.77 seconds).
The host-helper suite reports 6 passed and the one total-check failure (1.12 seconds).
No corpus source, output golden, or LIR snapshot changed in this partial round.

### Test-move account

The `compiler/src/lib.rs` and `codegen/src/ship.rs` test moves occurred in the same
working tree as the implementation. They were not isolated implementation-free commits.
The compiler body changes are the three named above.
The ship test bodies now changed by the header migration are:

- `module_state_is_isolated_between_concurrent_contexts_in_both_tiers`
- `ship_c_host_trap_observer_and_clear_api_preserve_unwind_semantics`
- `ship_c_corpus_output_is_byte_identical_with_an_observer_registered`
- `host_memory_accounting_agrees_on_count_and_measures_tier_bytes`
- `collect_drops_dead_lir_temporaries_on_both_tiers`
- `host_allocation_attribution_reports_known_sites_on_both_tiers`
- `allocation_corpus_object_request_counts_match_across_tiers`

Their shared C compilation helper also writes the program header.
The missing-main CLI test moves from commands.rs to watch.rs to use its process capture helper.


## Third Phase Review resumed with interop scope

The owner authorized the corpus interop C sources and their build harnesses.
`interop.c` and `wire-enum.c` now include the emitted program header.
Their handwritten entry declarations and weak entry definitions are removed.
The generated header supplies `SUBSCRIPT_HOST_ENTRY_<name>` flags from the host table.
These flags select only the fixture drivers whose program exports the required entry.
The static dev fixture builds library functions only, with `SUBSCRIPT_INTEROP_LIBRARY_ONLY`.
Both native-fixture and capture build scripts select that form.
Ship compilation adds the generated directory to the native sources' include search.

The CLI watch test's closure result now has an explicit type.
All 22 command tests pass (3.32 seconds); all 11 watch tests pass (0.85 seconds).
The watch test compares run and watch with a294 and a297's existing goldens.
Its edit control changes zeta's body and observes the changed output after a swap.
The missing-main test compares all four diagnostics, observes that watch stays alive,
and edits the program to a valid main that runs.

The reload runner has a direct test for initial execution and a changed generation.
A missing-main control rejects the call before any async root runs.
The helper test compiles an argument-taking main with its emitted header.
Removing the parameter from that header independently fails both host and program compilation.
The file-emission test also checks a custom header label in program C and the AOT entry.
The total check scans C sources and decoded Rust string literals, with injected declaration controls.
It has no fixture exemption. The helper suite passes all seven tests (1.12 seconds).
Host-entry tests pass all 12 tests (4.77 seconds in the preceding partial run).
Documentation tests pass all eight tests (0.64 seconds in the preceding partial run).

No new corpus entry or TypeScript source change is required in this resumed round.
The preceding total-check failure is the measured Red for the interop declaration migration.
The acceptance-one measurement and TypeScript measurements remain as recorded above.


### Completed third-review checks

The full codegen test run reports 645 passed, zero failed, and one ignored.
The sum of its suite times is 348.75 seconds.
C emission reports 105 passed (67.54 seconds); golden comparisons report 35 passed (127.22 seconds).
LIR reports 53 passed (9.11 seconds). The ignored full interpreter sweep runs in the release gate.
The compiler accept, reject-table, and host-entry suites pass.
The final total-check and helper suite passes seven tests (1.12 seconds).

The release benchmark binary build passes (20.65 seconds).
All three release benchmark host compilation controls pass (0.09 seconds each).
The capture-interop feature build passes (6.67 seconds).
The LIR generator passes (1.94 seconds); its complete output equals the current snapshot byte for byte.
There are no added, changed, or removed snapshot ranges in this round.
No existing output golden changed.

Pinned formatting passes. The offline locked workspace all-target build is warning-free (14.85 seconds).
The designated generator refreshed the documents.
Clippy counts remain 3/18/13 (5.32 seconds), with no new warnings in the changed tests.
Hygiene is the final check before the one authorized full gate.
No edits or other checks run during that gate. Its generated record and Japanese report carry the verdict.
