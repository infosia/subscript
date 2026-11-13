# Associative iteration owner

## Red measurement

At `72a3bae4`, `map_iteration_exit_keeps_storage_bounded` fails.
The test traverses four Map entries, then runs 100,000 delete/insert pairs.
The output is `4`.
Live bytes are 4,194,481. Reserved bytes are 4,194,545.
The test requires fewer than 65,536 reserved bytes.
The test takes 35.41 seconds; the Cargo command takes 37.762 seconds.
`cargo fmt --check` passes in 2.070 seconds.

## Reduced Red test

At `72a3bae4`, the test uses 2,000 delete/insert pairs and four live entries.
It requires fewer than 32,768 reserved bytes.
The test fails with 65,713 live bytes and 65,777 reserved bytes.
The first run takes 0.07 seconds; its Cargo command takes 3.48 seconds, with compilation.
The second run takes 0.04 seconds; its Cargo command takes 0.14 seconds.
`cargo fmt --check` passes in 1.95 seconds.
The Step B and Step C tests pass the 32,768-byte reserved-storage bound.

## Implementation status

The implementation checks below use commit `eebd6279` with the task changes in the working tree.

The sections below record the implementation and its checks.
Step C is complete.

The handoff records these earlier form measurements:

| Form | Sites | Files | Allocation per loop |
| --- | ---: | ---: | ---: |
| Internal `Type` variant | 22 | 12 | 1 |
| Counted iterator cursor | about 16 | 11 | 0 |
| Third form | No smaller form found | — | — |

These numbers come from the handoff, not from this test run.

## Step B: tier and frame release

Each tier calls iteration_begin once per Map/Set cursor creation.
The new iteration_bound function reads the ordered bound without a new visit.
The existing fixed and live bounds keep their corpus outputs.

Generator saves keep ValueType owners in the interpreter and native cleanup descriptions.
Description node 6 ends the cursor visit during frame release.
An advance preserves the same owner in the generator count check.
Async saves transfer the cursor bytes; completion uses the normal lexical release.
An unfinished async frame retains its existing scheduler lifetime under rule 8.
Map and Set payloads have no container count action; the cursor adds no second container count.
No cursor allocation is added.

The tests cover normal exit, break, return, caught throw, and generator drop after one next().
An async test suspends inside a visit and checks storage after completion.
Each program checks the 32,768-byte bound in the JIT and output equality in the interpreter and C AOT.
The runtime test checks nested depths 2, 1, and 0, plus bound reads without a depth change.
The reduced Red result above remains the pre-change measurement.

Step B changes these files:

- codegen/src/cemit/generator_cleanup.rs
- codegen/src/cemit/graph.rs
- codegen/src/cemit/iterator.rs
- codegen/src/generator_cleanup.rs
- codegen/src/interpreter.rs
- codegen/src/interpreter/counted.rs
- codegen/src/interpreter/operations.rs
- codegen/src/jit/symbols.rs
- codegen/src/lir/verify_generator_counts.rs
- codegen/src/lower/func/instruction.rs
- codegen/src/lower/func/iterator.rs
- codegen/src/lower/mod.rs
- codegen/tests/assoc_iteration_owner.rs
- runtime/src/assocops.rs
- runtime/src/assocops/iteration_tests.rs
- runtime/src/context/counted.rs
- runtime/src/ffi/associations.rs
- specs/tracking/assoc-iteration-owner.md

The existing Step A changes remain in the tree.
The corpus entry and LIR text snapshot remain for Step C.

Final checks use `/usr/bin/time -p`; times include Cargo startup and compilation.

| Check | Result | Wall seconds |
| --- | --- | ---: |
| Workspace build | PASS | 18.78 |
| Growth and exit tests (3 passed) | PASS | 4.27 |
| LIR tests, text snapshot excluded (66 passed) | PASS | 10.29 |
| Release model tests (6 passed) | PASS | 0.11 |
| Interpreter unit tests (62 passed) | PASS | 23.04 |
| Runtime unit tests (406 passed, 1 ignored) | PASS | 2.44 |
| cargo fmt --check | PASS | 2.12 |

## Step C: visit bound measurement

Node, the interpreter, JIT, and C AOT visit the entry added during the visit.
The output includes `insert:4`, after `insert:3`.
The expected output at that measurement omits this line, so the differential test fails.
`lower_for_of` in `codegen/src/lir/stmt.rs` selects `IteratorBoundKind::Live`.
The LIR snapshot diff at that measurement has zero files, zero lines, and zero blocks.

Step C changes these files:

- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.ts
- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.expected
- codegen/tests/assoc_iteration_owner.rs
- specs/tracking/assoc-iteration-owner.md

| Check | Result | Wall seconds |
| --- | --- | ---: |
| Project tsc: `node node_modules/typescript/bin/tsc --project tsconfig.json` | PASS | 0.93 |
| Node: TypeScript transpileModule, CommonJS execution, call main | Extra `insert:4` | 0.14 |
| a444 differential, first interpreter assertion | FAIL | 2.65 |
| a444 differential, all three tiers before expected comparison | Tier equality PASS; expected FAIL | 2.75 |
| cargo fmt --check | PASS | 2.01 |

The final differential test body takes 0.56 seconds; its command takes 2.75 seconds with compilation.
The earlier Red numbers and three-form measurement remain above.
Acceptance item 6 retains the Step B source inspection: cursor fields use the existing value storage.
The new count action adds begin/end calls, with no allocation call or new runtime owner object.
Array cursors have no new count action. This is source evidence, not a new allocation measurement in Step C.


## Step C: snapshot scope measurement

Rule 4 and acceptance item 4 now require the live insertion bound.
The a444 expected output includes `insert:4`; its header states that added entries are reached.
Project tsc passes in 0.66 seconds.
Node transpiles the entry, runs main, and matches the expected bytes in 0.13 seconds.
The iteration_begin comment now distinguishes the fixed forEach/spread bound from the live for-of bound.

The capture command uses `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1` with the named snapshot test.
It passes in 7.18 seconds; the test body takes 4.25 seconds.
The generated candidate touches only a146, a224, a341, a427, a432, and a444.
However, a146 changes more than cursor count instructions.
It adds a cursor exit parameter, changes ownership flags, and renumbers subsequent values.
It also redirects a call exception to a new release.landing block.
That block parks the exception, releases the cursor, and resumes the exception.
The handoff permits cursor count instructions only and requires a stop on other changes.
The candidate stays in temporary storage; the committed snapshot remains unchanged.
Its applied diff is zero lines and zero blocks.

The registry generator and remaining checks did not run after this stop.
The three-tier a444 check, LIR suite, reference checks, new tests, fmt, and clippy remain required.
The acceptance item 6 evidence remains the Step B source inspection above.
This check adds no allocation measurement.
The earlier Red numbers and three-form measurement remain above.

The snapshot scope measurement changes these files:

- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.ts
- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.expected
- runtime/src/assocops.rs
- specs/tracking/assoc-iteration-owner.md

No background task remains from this check.

The candidate diff has 1,270 added lines and 650 removed lines:

| Block | Added | Removed |
| --- | ---: | ---: |
| a146 | 46 | 39 |
| a224 | 67 | 63 |
| a341 | 308 | 296 |
| a427 | 179 | 162 |
| a432 | 100 | 90 |
| a444 | 569 | 0 |


## Step C: completion

Acceptance item 7 permits the measured branches, exception edges, and value numbers in the six specified entries.
`SUBSCRIPT_CAPTURE_LIR_GOLDENS=1` captures the snapshot through `coroutine_and_measurement_lir_text_matches_goldens`.
The capture writes `std::env::temp_dir()/subscript-lir-goldens.txt`; its bytes replace the snapshot without manual edits.
A block comparison against the previous snapshot finds exactly these entries:

- a146-scoped-locals
- a224-generator-as-a-map-value
- a341-reference-holder-release
- a427-named-function-takes-a-void-result-place
- a432-a-const-local-function-value-takes-an-adapter-that-captures-it
- a444-a-map-or-set-iteration-ends-with-its-owner

The snapshot diff adds 1,270 lines and removes 650 lines.
The LIR suite accepts every corpus entry through the verifier and the independent HIR release model.
The suite includes all six release-model tests, including the missing exception-edge cursor release and its control.
The interpreter corpus profile passes its existing output goldens.
a444 passes its expected bytes in the interpreter, JIT, C AOT, and Node.
No existing output golden changes.

`generate-api-reference` regenerates all three reference documents.
Only `generated-docs/corpus-index.md` changes: it gains the a444 row.
The generated language and API references remain byte-identical.

Acceptance item 6 uses source comparison against HEAD, rather than a new allocation benchmark.
The cursor keeps the existing value storage in each tier and the existing generator/async frame slots.
The new action calls iteration_end; begin changes the existing header depth, and bound reads the existing order length.
None of these paths allocates a runtime object or adds an allocation call.
Map and Set storage has no recursive container count, so the cursor adds no second container count.
Array cursors remain Uncounted; the array iteration paths do not change.
The growth and exit tests independently confirm bounded reserved storage after cursor release.

The completion changes the LIR snapshot, generated corpus index, and this tracking note.
The full task file list follows below.
Times use `/usr/bin/time -p` and include compilation and Cargo lock waits.

| Check | Result | Wall seconds |
| --- | --- | ---: |
| LIR snapshot capture | PASS, 1 test | 4.72 |
| Reference generator | PASS | 0.64 |
| LIR suite, verifier and release model | PASS, 67 tests | 12.05 |
| New growth, exits, async completion, a444 differential | PASS, 4 tests | 5.00 |
| Generated reference unit tests | PASS, 4 tests | 3.53 |
| Runtime unit tests | PASS, 406 tests, 1 ignored | 2.38 |
| Codegen unit tests | PASS, 344 tests | 54.43 |
| Project tsc | PASS | 0.71 |
| Node a444 byte comparison | PASS | 0.15 |
| cargo fmt --check | PASS | 2.44 |
| Workspace clippy, all targets | PASS with existing warnings | 17.13 |

Clippy reports five warnings in changed files, all on unchanged source lines:
`runtime/src/assocops.rs:845` and `codegen/src/cemit/iterator.rs:6` exceed the argument-count lint.
`codegen/src/lower/mod.rs:20`, `:21`, and `:22` trigger the documentation-list indentation lint.
No new changed-line warning appears.
The task does not run `tools/gate.sh`.

### Full task file list

- codegen/tests/assoc_iteration_owner.rs
- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.expected
- corpus/accept/a444-a-map-or-set-iteration-ends-with-its-owner.ts
- runtime/src/assocops/iteration_tests.rs
- specs/tracking/assoc-iteration-owner.md
- codegen/src/cemit/generator_cleanup.rs
- codegen/src/cemit/graph.rs
- codegen/src/cemit/iterator.rs
- codegen/src/generator_cleanup.rs
- codegen/src/interpreter.rs
- codegen/src/interpreter/counted.rs
- codegen/src/interpreter/instruction.rs
- codegen/src/interpreter/operations.rs
- codegen/src/jit/symbols.rs
- codegen/src/lir.rs
- codegen/src/lir/builder.rs
- codegen/src/lir/verify_counted_operations.rs
- codegen/src/lir/verify_generator_counts.rs
- codegen/src/lir/verify_instruction.rs
- codegen/src/lower/func/instruction.rs
- codegen/src/lower/func/iterator.rs
- codegen/src/lower/mod.rs
- codegen/tests/lir-goldens/corpus.txt
- codegen/tests/support/lir_facts_release.rs
- compiler/src/lir.rs
- generated-docs/corpus-index.md
- runtime/src/assocops.rs
- runtime/src/context/counted.rs
- runtime/src/ffi/associations.rs
