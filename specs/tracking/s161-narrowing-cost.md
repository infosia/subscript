# §161 — Narrowing cost

Contract: `specs/blocks/compiler/s161-narrowing-cost-follows-the-live-facts.md`.
Contract pin: `3d380bdd`.
Date: 2026-10-05. Machine architecture: arm64. Rust: 1.95.0.

## Method

The release build command was `cargo build --offline --locked --release -p subscript-cli`.
The pin CLI came from a detached worktree under `$TMPDIR`.

The generator accepts an output directory and `--sizes`.
It emits `distinct`, `narrowed`, `blocks`, and `generic`.
It also emits the controls `same` and `generic_i32`.
The round 2 generator forms passed the pin CLI at size 10 before the production changes.
All four final forms and both controls pass the pin CLI at size 10.
All measured sizes also passed the pin CLI.
The output uses `print` with a template literal.

Each timing is the smallest elapsed time of three serial `subscript check` processes.
Python `time.perf_counter()` measured the full CLI process, including file loading and HIR destruction.
The round 3 timing runs followed the final release build and completed compiler and codegen tests.
The table uses these runs for all Pin and After cells, including both controls.
Earlier runs that overlapped other work do not supply the table.
All sources for the final timing and comparison runs stayed under `$TMPDIR`.

The round 2 counters below measure the narrowing change before round 3.
The sandbox denied the sample profiler's process access.
Temporary release builds used nested `Instant` guards in the named bookkeeping functions.
A guard around `check_program_with` supplied the denominator, including parsing and both checker passes.
The union counter measured the outermost named intervals, so nested intervals counted once.
Each profile used the trial with the smallest denominator from three serial runs of `blocks3000`.
Individual inclusive shares overlap. Timer guards add overhead.
The measurement scripts removed the temporary worktrees after the measurements.
No timing counter remains in the production source.

## Form and consumers

`NarrowingFact` stores the path and its shared-location flag.
The leaf condition or non-null assignment supplies the flag from its target expression.
Live, ended, and hidden facts use this form.
The checker removes `Scope::shared_narrowing_paths`, its accessor, and `register_shared_paths`.
No path registry remains for unrelated conditions or stores.

The consumer audit covered `narrowing_paths`, `apply_narrowing_effects`, `FnCtx::declare`, and `FnCtx::pop_scope`.
These were all consumers of the old shared-path registry and hidden-fact map.
Each consumer now reads the flag from its fact.
No consumer needs the shared flag of a path without a fact.

Fact sets use `BTreeSet`, so a scan does not visit the capacity of an earlier, larger hash set.
A separate index names only scopes that contain hidden facts.
The empty-state test is constant, even with many empty scopes.
If no fact is live, the checker skips statement, handler-entry, and loop effects.
A statement records only top-level disposal declarations, including its synthetic prefix, in its scope.
Scope exit reads that flag without a body scan.
Each switch case starts with its own disposal flag.
The try effect summary is absent only when its incoming, outgoing, and hidden fact sets are empty.

The checker borrows the module's class list and helper set.
The provisional helper registry records each helper when the checker creates it.
The opaque snapshot restores that registry with its function list.
No point copies or reconstructs the helper set.

Two other costs prevented the timing acceptance after the path-registry change.
Statement diagnostic suppression scanned all locals.
Each declaration now records whether its apparent type is `Type::Error` under the body check substitution.
`Scope::insert_local` updates that status when it replaces a local.
Each statement saves shared references to the scopes' rejected-name sets.
A lookup reads only the requested name from those sets.
No statement copies or scans local names.

The substitution audit covered generic instantiation, initializer decisions, deferred expressions, and deferred bodies.
Generic instantiation sets the substitution and applies opaque constraints before it checks the body.
Nested opaque instances check constraints but do not update the root body's constraints.
Initializer and deferred-body checks restore their saved substitutions.
The apparent type does not change between statements of one body check.
Parameter decisions replace local entries before the delayed default body runs.
That replacement also updates each deferred parameter scope's rejected-name set.
No other body point needs to rebuild the index.

Fact-set insert, replace, and extend operations assert that facts with one key have the same shared-location flag.
The capture analysis copied all local bindings at each branch.
A shared snapshot now copies the environment only on a write.
Before that capture change, counters measured 222.215 ms inside `capture::check` from a 328.308 ms check of `distinct4000`.

## Release CLI timings

Times are seconds. Each cell is the smallest of three elapsed times.

| Form | n | Pin | After |
| --- | ---: | ---: | ---: |
| distinct | 1,000 | 1.977105 | 0.033376 |
| distinct | 2,000 | 8.533415 | 0.061864 |
| distinct | 4,000 | 32.181793 | 0.117497 |
| narrowed | 1,000 | 3.593014 | 0.051788 |
| narrowed | 2,000 | 14.626493 | 0.102086 |
| narrowed | 4,000 | 59.460405 | 0.198523 |
| blocks | 1,000 | 0.678782 | 0.392291 |
| blocks | 2,000 | 1.506750 | 0.808271 |
| blocks | 3,000 | 2.542911 | 1.232740 |
| blocks | 4,000 | 3.704240 | 1.693076 |
| same | 1,000 | 0.032777 | 0.026976 |
| same | 2,000 | 0.061242 | 0.049974 |
| same | 4,000 | 0.117044 | 0.095294 |
| generic | 1,000 | 0.086200 | 0.013080 |
| generic | 2,000 | 0.316938 | 0.021573 |
| generic | 4,000 | 1.223076 | 0.037840 |
| generic_i32 | 1,000 | 0.023133 | 0.010615 |
| generic_i32 | 2,000 | 0.071472 | 0.018144 |
| generic_i32 | 4,000 | 0.261758 | 0.032254 |

Acceptance ratios:

- `distinct4000 / distinct1000`: 3.520; limit 5.
- `narrowed4000 / narrowed1000`: 3.833; limit 5.
- `distinct4000 / same4000`: 1.233; limit 1.5.
- `blocks3000 after / pin`: 0.485; limit 1.
- `generic4000 / generic1000`: 2.893; limit 5.
- `generic4000 / generic_i324000`: 1.173; limit 1.5.

## Round 2 bookkeeping counters at blocks n = 3,000

| Counter | Pin ms | Pin share | After ms | After share |
| --- | ---: | ---: | ---: | ---: |
| check_program_with | 2482.543 | 100.00% | 1119.864 | 100.00% |
| Named interval union | 1404.127 | 56.56% | 107.530 | 9.60% |
| `end_shared_narrowing` | 1259.516 | 50.73% | 25.767 | 2.30% |
| `FnCtx::shared_narrowing_paths` | 568.209 | 22.89% | 0.000 | 0.00% |
| `narrowing_paths` | 25.004 | 1.01% | 1.556 | 0.14% |
| `apply_narrowing_effects` | 667.409 | 26.88% | 2.032 | 0.18% |
| `register_shared_paths` | 42.263 | 1.70% | 0.000 | 0.00% |
| `narrowing_helpers` | 471.561 | 19.00% | 0.794 | 0.07% |
| `FnCtx::declare` | 47.623 | 1.92% | 13.515 | 1.21% |
| `FnCtx::pop_scope` | 6.818 | 0.27% | 6.955 | 0.62% |
| `end_loop_narrowing` | 4.345 | 0.18% | 0.114 | 0.01% |
| `end_scope_narrowing` | 1.161 | 0.05% | 1.158 | 0.10% |

The pin rebuilt shared-path sets 939,014 times and registered paths 228,000 times.
Both counts are zero after the change.
`apply_narrowing_effects` calls fell from 939,014 to 36,000.
`narrowing_helpers` calls fell from 936,014 to 36,000 and now borrow the set.

## Equivalence and tests

The CLI comparison checked complete stdout, stderr, and exit status for 860 distinct files.
All outputs were identical.
The count includes 811 corpus files, 20 §160 generated fixtures, and six §161 size-10 fixtures.
It also includes 19 timing inputs and four error-local inputs.
No checked-in `.ts` fixture exists under `compiler/tests`; the fixture comparison covers both committed generators.
Each timing input also produced identical output across its three trials.
The `generic_i32` control keeps the generic signature and return, but replaces each local with `const xI: i32 = 1`.
The error-local inputs compare an unknown local type inside generic and concrete functions, with immediate reads and reads after other statements.
The pin and final CLI each report only S016 for the unknown type in those four inputs.
The pin and corrected CLI accept the nested-disposal program and reject its direct-disposal control with S011 after the outer block.
No corpus source, output, or golden changed.

Five new tests in `compiler/tests/shared_narrowing.rs` each include a control:

- A call or alias store ends a shared fact but preserves a local fact.
- Scope exit restores a hidden fact after 128 unrelated conditions; a script call ends the hidden shared fact.
- A right-operand call ends the left fact of `&&` and `||`; the constant control preserves it.
- Nested disposal preserves the later outer-scope fact; direct disposal ends it at scope exit.
- An unknown local type in a generic body preserves diagnostic suppression after other statements; a `T`-typed control passes.

Required checks passed:

- `cargo test --offline --locked -p subscript-compiler`: 1,009 tests passed, including doctests.
- `cargo test --offline --locked -p subscript-codegen`: 726 tests passed.
- `cargo fmt --check`: passed.
- `cargo clippy --offline --locked --workspace --all-targets`: passed; all 71 warning sites match the pin.
- `tools/gate.sh full`: `debug 2332/0/3 release 2329/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

Every changed Rust file has fewer than 2,000 lines.
The `cfg(test)` module contains the condition-test helper and its `BinOp` import.

## Downstream check

The measurement uses the source sets of the 55 subscript-typegpu programs at commit `31f9ca9` of that repository: the library modules, the program, and its generated support module.
A debug build of `check_program`, best of three calls per program, gives the sum: 19.44 s at `de41409`, 22.07 s at `a502cf1d`, 14.43 s after §161.
All 55 programs pass at each pin.

## Changed files

- `compiler/src/check/bindings.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/initializer.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/local_errors.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/narrowing_fact.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/tests/fixtures/s161_gen.py`
- `compiler/tests/shared_narrowing.rs`
- `specs/tracking/s161-narrowing-cost.md`
