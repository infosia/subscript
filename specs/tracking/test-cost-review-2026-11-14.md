# Test cost review, 2026-11-14

Source: a test cost review at `8a36c8c1`. The baseline is the full gate
record of 2026-11-13 at `8a36c8c1`, on `aarch64-apple-darwin`:

```text
gate full 8a36c8c1cd5a35841b7b89a0464b42198d2e74df clean debug 3418/0/5 release 3415/0/5 skips 2/0 clippy 3/10/13 goldens-moved 0 exit 0
```

| Step | Wall seconds |
|---|---:|
| fmt | 2 |
| build | 55 |
| debug tests | 589 |
| release tests | 589 |
| clippy | 21 |
| tsc | 1 |
| hygiene | 3 |

The largest suites in that record:

| Suite | Debug s | Release s |
|---|---:|---:|
| `codegen/tests/lir.rs` | 9.03 | 173.57 |
| `cli/tests/gate.rs` | 46.27 | 46.01 |
| `codegen/tests/golden.rs` | 42.88 | 27.28 |
| `examples/tests/gate.rs` | 59.96 | 4.47 |
| `subscript-codegen` library tests | 45.56 | 9.80 |
| `compiler/tests/generic_tsc_matrix.rs` | 34.72 | 16.75 |
| `codegen/tests/boundary_scratch_breadth.rs` | 36.57 | 2.38 |
| `codegen/tests/long_string_constants.rs` | 33.42 | 3.90 |
| `compiler/tests/robustness.rs` | 29.13 | 3.36 |

## §276 — the gate script tests

The contract pin is `2165c68ae08fd089990a7d92310c9913cf51d2b2`.
The coding agent built the test binary before each debug measurement.

| Check | Command | Result | Wall seconds |
|---|---|---|---:|
| Before the change | `cargo test -p subscript-cli --test gate` | 14 passed; 0 failed | 45.30 |
| Red control at the pin | `cargo test -p subscript-cli --test gate full_fails_on_hygiene_failure -- --exact` | 0 passed; 1 failed; exit 101 | 5.15 |
| After the change | `cargo test -p subscript-cli --test gate` | 16 passed; 0 failed | 16.60 |
| Release | `cargo test -p subscript-cli --test gate --release` | 0 tests; no `gate-skip:` line | 0.51 |
| Format | `cargo fmt --check` | Exit 0 | 2.09 |
| Clippy | `cargo clippy -p subscript-cli --all-targets` | Exit 0; dependency warnings | 11.48 |

`/usr/bin/time -p` measured each wall time.
The red control added only the new case to the pin.
The gate script still used the real hygiene command at that point.
The output was:

```text
running 1 test
test full_fails_on_hygiene_failure ... FAILED

assertion `left == right` failed
  left: Some(0)
 right: Some(1)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 14 filtered out; finished in 4.57s
error: test failed, to rerun pass `-p subscript-cli --test gate`
real 5.15
user 3.55
sys 0.75
```

Each script case now supplies a hygiene stub.
The failure control checks exit 1 and the stub output in the hygiene block.
Case (d) checks the stub path and output after `tsc`, as the last step.
The text check permits the real hygiene path only as the default value.
The release test binary contains no tests.
The full gate result is below. The expected moves are debug +2 (the two new cases) and release −14 (the whole suite, by rule 5).

## §277 — the a22 interpreter run

The contract pin is `2d829b85423d2078884976af331d0fac68d48a2f`.
The agent built the release LIR binary before the baseline run.
`/usr/bin/time -p` measured the wall times.
Both release measurements set `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1`.
The final release measurement ran after all other checks stopped.

| Check | Result line | Wall seconds |
|---|---|---:|
| Release LIR before | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 177.42s` | 178.06 |
| Red interpreter sweep | `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 66 filtered out; finished in 184.36s` | 184.48 |
| Red golden sweep | `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.68s` | 30.22 |
| Release LIR after, concurrent checks | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.29s` | 4.45 |
| Release LIR after, final | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.11s` | 1.21 |
| Debug LIR | `test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.88s` | 53.73 |
| Golden sweep | `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 35.36s` | 35.56 |
| C emission sweep | `test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 25.70s` | 35.33 |
| Corpus suites | `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.65s` | 54.61 |
| Corpus suites | `test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s` | 54.61 |
| Corpus suites | `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s` | 54.61 |
| Corpus suites | `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s` | 54.61 |
| Language reference | `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 610 filtered out; finished in 0.12s` | 42.87 |
| Capture tool | Exit 0 | 0.64 |
| Document generator | Exit 0 | 37.30 |
| TypeScript, repository config | Exit 0 | 0.99 |
| Format check | Exit 0 | 11.47 |

The corpus rows share one command and its total wall time.
The debug, corpus, reference, and generator times include build lock waits.
The other final checks ran at the same time.
The red interpreter run failed because a448 had no golden.
The red golden sweep passed. It selects entries with existing goldens.
`compiler/tests/js_corpus.rs` reports a missing golden for each accept
entry. With the a448 golden moved aside, it failed with
`a448-matrix-propagation-witness.ts: missing golden`.

The capture tool produced `256.06827` and a final newline.
The golden sweep confirmed equal dev, ship, and golden bytes.
The interpreter sweep confirmed equal interpreter and golden bytes.
The witness body differs from a22 only in the two required constants.
The a22 golden did not change.
The debug LIR run printed no `gate-skip:` line.
The generator changed only the corpus index.
The index shows the a22 exclusion reason and a448 as `yes`.
The full gate result is below.

## Full gate after §276 and §277

One full gate ran on the §277 tree, above the §276 commit:

```text
gate full c93fbea123d3ed0a08ea25546ea5b0457254ff0a dirty:6 debug 3420/0/5 release 3401/0/5 skips 1/0 clippy 3/10/13 goldens-moved 0 exit 0
```

The counts move as stated: debug +2 and release −14 (§276), and the
debug skip count falls from 2 to 1 (§277). No golden moved.

| Step | Wall seconds, 2026-11-13 | Wall seconds, this gate |
|---|---:|---:|
| fmt | 2 | 2 |
| build | 55 | 3 |
| debug tests | 589 | 388 |
| release tests | 589 | 184 |
| clippy | 21 | 3 |
| tsc | 1 | 0 |
| hygiene | 3 | 4 |
| Sum | 1,260 | 584 |

The build state differs between the two gates. The earlier gate built
the release tests in its release step, and this gate found them built.
So only the suite times above are comparable, not the step sums.

## §278 — the golden sweep

Pin: `02aac875d69dcbaa221b76fbb2e4be135738ea2c`.

The coding agent read all 33 focused test bodies. Rule 1 removes each
test below.
Each test asserts only tier equality or equality with a golden.
The output prints give diagnostics. They add no assertion.
Each named entry has a golden. The reference host skips none.
Rule 3 keeps no focused test.

| Removed test | Entries |
| --- | --- |
| `r27_field_initializer_entries_match_across_tiers` | `a133-field-init-no-ctor`, `a134-field-init-order` |
| `r28_binary32_bit_access_matches_the_golden_across_tiers` | `a135-f32-bits` |
| `r29_class_index_signature_matches_the_golden_across_tiers` | `a136-index-signature` |
| `r30_handle_entry_parameters_match_the_golden_across_tiers` | `a137-handle-entry-param` |
| `r32_wire_entry_parameters_match_the_golden_across_tiers` | `a140-wire-entry-param` |
| `r31_using_disposal_matches_the_goldens_across_tiers` | `a138-using-dispose`, `a139-using-async` |
| `unicode_string_entry_matches_across_tiers_before_golden_comparison` | `a60-string-unicode` |
| `string_literal_union_entry_matches_across_tiers_and_golden` | `a91-string-literal-union` |
| `descriptor_literal_entry_matches_across_tiers_and_golden` | `a92-descriptor-literals` |
| `q34_async_entries_match_across_tiers_and_golden` | `a93-async-chain`, `a94-async-two-roots`, `a95-interop-async-await` |
| `r13_async_method_entries_match_across_tiers_and_golden` | `a110-async-method-receiver`, `a111-interop-async-method-poll` |
| `capturing_lambda_environment_survives_recursive_reentry` | `a114-lambda-env-recursion` |
| `scalar_parameter_pair_entry_matches_across_tiers_and_golden` | `a96-interop-byte-pairs` |
| `embedded_chain_header_box_keeps_the_complete_extension` | `a89-interop-chain-payload` |
| `managed_boundary_box_survives_the_building_function` | `a169-managed-boundary-box` |
| `string_field_pointer_write_direction_matches_both_tiers_and_golden` | `a97-interop-string-field-write` |
| `string_field_pointer_read_direction_matches_both_tiers_and_golden` | `a98-interop-string-field-read` |
| `texture_descriptor_write_direction_matches_both_tiers_and_golden` | `a99-interop-texture-descriptor-write` |
| `texture_descriptor_read_direction_matches_both_tiers_and_golden` | `a100-interop-texture-descriptor-read` |
| `recursive_boundary_pipeline_entries_match_both_tiers_and_goldens` | `a103-interop-recursive-compute-pipeline`, `a104-interop-recursive-render-pipeline`, `a105-interop-recursive-string-pair-elements` |
| `struct_pointer_recursive_boundary_pipeline_matches_both_tiers_and_golden` | `a106-interop-recursive-struct-pointer-members` |
| `handle_beside_arrays_through_nullable_member_matches_both_tiers_and_golden` | `a119-interop-handle-beside-arrays` |
| `nested_structs_behind_array_element_pointer_match_both_tiers_and_golden` | `a120-interop-nested-behind-element-pointer` |
| `unmarked_struct_pointer_in_array_element_matches_both_tiers_and_golden` | `a121-interop-unmarked-reach-through` |
| `two_reach_through_pointer_members_match_both_tiers_and_golden` | `a122-interop-two-pointer-members` |
| `wide_descriptor_breadth_and_depth_match_both_tiers_and_golden` | `a123-interop-wide-descriptor` |
| `contextual_conditionals_match_both_tiers_and_golden` | `a124-contextual-conditional` |
| `conditional_arm_narrowing_matches_both_tiers_and_golden` | `a125-conditional-arm-narrowing` |
| `suspension_state_matches_both_tiers_and_golden` | `a149-suspension-state` |
| `by_value_packing_matches_both_tiers_and_golden` | `a126-interop-by-value-packing` |
| `handle_parameter_pair_matches_both_tiers_and_golden` | `a107-interop-handle-parameter-pair` |
| `nullable_handle_parameter_matches_both_tiers_and_golden` | `a108-interop-nullable-handle-parameter` |
| `narrow_corpus_entries_match_across_tiers_before_golden_comparison` | `a46-narrow-numerics`, `a47-narrow-layout`, `a48-interop-narrow-slices`, `a49-f16-conversions`, `a50-narrow-callbacks-shifts` |

Keep `every_corpus_entry_with_a_golden_ends_in_a_newline` under rule 6.
It checks the last byte of each non-empty golden. It starts no run.
Keep the complete sweep. It compares both tiers with each golden.
No focused test has another observation under rule 2.
The phase list names no removed test. The dead-code check stays.
No support file changes. No helper becomes unused.

Add `abnormal_termination_failure_retains_stdout` to the parallel phase.
It checks the failure formatter with abnormal-termination text and stdout
`x\n`. The failure line must contain `output before termination:\nx\n`.
The dev error branch passes the retained stdout to this formatter.
Keep the host hooks, file provider, entry arguments, and exclusions.

Build both test binaries before each timed suite run.
Measure wall time around the Cargo command. Exclude the build command.
The harness hides successful phase-2 output. Run the sweep again in its
phase-2 child mode to read its counts. Keep these times separate.

Before the change:

- Debug wall time: 44.076 s.
  `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 43.24s`
- Release wall time: 28.319 s.
  `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.21s`
- Direct debug sweep wall time: 34.615 s.
  `golden sweep: compared 436 entries, skipped 0 entries`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 34.61s`
- Direct release sweep wall time: 22.726 s.
  `golden sweep: compared 436 entries, skipped 0 entries`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 22.72s`

After the change:

- Debug wall time: 36.225 s.
  `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 35.46s`
- Direct debug sweep wall time: 35.156 s.
  `golden sweep: compared 436 entries, skipped 0 entries`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 35.15s`
- Release wall time: 19.509 s.
  `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.01s`
- Direct release sweep wall time: 18.826 s.
  `golden sweep: compared 436 entries, skipped 0 entries`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 18.82s`

Both profiles compare the same 436 entries and skip 0 entries.
The suite removes 33 tests and adds one control. It now has 3 tests.
Debug wall time falls by 7.851 s. Release wall time falls by 8.810 s.
These are single runs. The direct sweep times are separate measurements.

`cargo fmt --check` passes.
`cargo clippy -p subscript-codegen --all-targets` passes.
Clippy reports warnings in unchanged code. It reports none in the changed test.
`git diff --check` passes. No golden changes.
The control of rule 4 builds the failure line from a stdout value. It
does not reach the extraction in `dev_sweep_run`, because a test
outside the crate cannot build an `AbnormalTermination`. The full gate
result is below.

Full gate on the §278 tree:

```text
gate full 02aac875d69dcbaa221b76fbb2e4be135738ea2c dirty:3 debug 3388/0/5 release 3369/0/5 skips 1/0 clippy 3/10/13 goldens-moved 0 exit 0
```

The counts move by −32 in each profile: 33 removed tests and one new
control. The debug step took 457 s and the release step 258 s. Both
steps rebuilt the changed test binary, so these step times do not
compare with the earlier gate.

## §279 — the frame-root corpus check

The contract pin is `35f009b0`.
The test uses a bounded worker pool. Each item returns its findings and counts.
The test thread reports findings in corpus order. The control injects one finding and checks the failure text.
The test module contains all new helpers. No production module changes.
The test thread loads the sources before the pool starts. Each worker owns its compiler state and JIT module.
The test comments state the shared tables and thread-local captures.

Build both profiles before each measurement. Measure wall time around the Cargo command. Exclude the build command.

Before the change:

- Debug wall time: 36.68 s.
  `frame parity: 598 coroutines, 1015 suspension points, both native tiers`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 352 filtered out; finished in 35.79s`
- Release wall time: 4.29 s.
  `frame parity: 598 coroutines, 1015 suspension points, both native tiers`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 352 filtered out; finished in 3.78s`

After the change:

- Debug wall time: 13.39 s.
  `frame parity: 598 coroutines, 1015 suspension points, both native tiers`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 353 filtered out; finished in 12.66s`
- Release wall time: 1.85 s.
  `frame parity: 598 coroutines, 1015 suspension points, both native tiers`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 353 filtered out; finished in 1.37s`

Both profiles retain 598 coroutines and 1,015 suspension points. The control adds one library test.
These measurements use single runs.

The full debug library suite passes. Wall time: 23.92 s.
`test result: ok. 354 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 23.82s`
`test lower::func::frame_root_tests::entry_finding_fails_with_its_id ... ok`

`cargo fmt --check` passes. Wall time: 2.03 s.

`cargo clippy -p subscript-codegen --all-targets` passes. Wall time: 3.35 s.
`Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.25s`
Clippy reports warnings in unchanged code. It reports none in the changed test.

`git diff --check` passes. No golden changes. The requested checks exclude the full gate.

Review corrections:

The shared pool runner catches each entry panic and returns its payload text with the entry ID.
The worker continues with its other entries. The runner returns all reports in corpus order.
Both controls use this runner with 17 entries. One check returns an error. The other check panics.
Each control checks all report IDs and the failure text. Equality findings print both values with Debug.

Build the debug test binary before the corpus measurement. Exclude the build from the measurement.

- Debug corpus wall time: 13.300 s.
  `frame parity: 598 coroutines, 1015 suspension points, both native tiers`
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 354 filtered out; finished in 12.59s`
- The broad `entry_` control filter wall time: 0.115 s.
  `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 340 filtered out; finished in 0.02s`
- The two frame-root controls wall time: 0.179 s.
  `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 353 filtered out; finished in 0.00s`
  The failures name `control-1: injected finding` and `control-1: panic: injected panic`.
- The full debug library suite wall time: 24.612 s.
  `test result: ok. 355 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 24.51s`
- `cargo fmt --check` passes. Wall time: 2.032 s. Exit code: 0.
- `cargo clippy -p subscript-codegen --all-targets` passes. Wall time: 3.631 s.
  `Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.53s`
  Clippy reports warnings in unchanged code. It reports none in the changed test.

The counts remain 598 coroutines and 1,015 suspension points. The panic control adds one library test.
`git diff --check` passes. The source file contains 1,101 lines. Only the two permitted files change.
No golden changes. These checks exclude the full gate. No commit is made.

Full gate on the §279 tree:

```text
gate full 35f009b0ddffa3c8ff48ead9f74641563372a963 dirty:3 debug 3390/0/5 release 3371/0/5 skips 1/0 clippy 3/10/13 goldens-moved 0 exit 0
```

The counts move by +2 in each profile: the two pool controls.

## §280 — the example host tests

