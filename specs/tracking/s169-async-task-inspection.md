# §169: Async task inspection

## Contract pin

The amended contract pin is `d8c878bd`. The round 3 implementation pin is `681279c5`. The earlier position-table probe used `049a0d6b`.

## Interpreter position facts

Rule 7 requires the exact suspension position for a parked invocation.
Rule 10 requires the interpreter to report the same task fields as the production tiers.

The interpreter's `AsyncRequest::Park` has no position field (`codegen/src/interpreter.rs`, lines 306–313).
The `Call` and `Handle` variants carry a `Pos`; the `Park` variant does not.

The LIR `Terminator::Suspend` carries `pos`.
The interpreter discards that position when it constructs `AsyncRequest::Park` (line 1145).
It then replaces `Frame.block` with the successor block (line 1202).
The returned `Flow::Suspended` carries only the yielded value and the request (line 1205).
The scheduler stores only the coroutine on `async_parked` (lines 877–882).
Neither `Coroutine` nor `Frame` has a suspension-position field (lines 268–335).

A successor identifies the next block, not the executed suspension.
Two suspension edges can share a successor. A reverse graph search cannot select the executed edge.
A function with no locals also retains no local invalidation position.

These types are interpreter implementation state, not the input form.
The LIR carries the required fact. The round 1 form-defect claim was incorrect.
A position field on the park request and a retained coroutine position fit the authorized file set.
ACTIVE and COMPLETE records must report position id 0.

## Position mappings

The tiers do not assign the same position ids at the contract pin.
Each tier builds its own `PositionTable` from the sites that its lowering visits.
`PositionTable::add` appends every script site, including repeated sites (`codegen/src/position_table.rs`, lines 58–64).
The table reserves id 0 for the empty position.

The JIT uses `ModLower::pos_id` (`codegen/src/lower/mod.rs`, lines 378–380).
`FuncLower::position_id` calls it (`codegen/src/lower/func/value.rs`, lines 365–367).
The C tier uses `Emitter::pos_id` (`codegen/src/cemit/emitter.rs`, lines 66–68).
Both call `PositionTable::add`, but their visits differ.
The interpreter retains source `Pos` values and has no equivalent position-id table.

A temporary unit probe compiled the following source through both lowerings at `049a0d6b`:

```ts
export async function main(): Promise<void> { const xs: i32[] = [7]; await Context.suspend(); print(`${xs[0]}`); }
export async function other(): Promise<void> { await Context.suspend(); }
```

The file name was `test.ts`. The finished tables gave these mappings:

| Position id | Dev JIT position | C AOT position |
| --- | --- | --- |
| 0 | empty, 0:0 | empty, 0:0 |
| 1 | test.ts:1:23 | test.ts:1:23 |
| 2 | test.ts:1:65 | test.ts:1:65 |
| 3 | test.ts:1:66 | test.ts:1:66 |
| 4 | test.ts:1:104 | test.ts:1:104 |
| 5 | test.ts:1:104 | test.ts:1:104 |
| 6 | test.ts:1:104 | test.ts:2:23 |
| 7 | test.ts:2:23 | absent |

The async function `other` therefore has function position id 7 in the JIT and 6 in C.
Rule 6 reports this frame allocation position. Acceptance 3 requires equal task records, so this difference affects the required output.

The same-shape control keeps two async exports and removes the array access:

```ts
export async function main(): Promise<void> { await Context.suspend(); }
export async function other(): Promise<void> { await Context.suspend(); }
```

Both tables give id 1 to `test.ts:1:23` and id 2 to `test.ts:2:23`.
Both retain the reserved id 0.

The command was `cargo test --offline --locked -p subscript-codegen --lib s169_position_mapping_probe -- --nocapture`.
The probe passed and printed both independently built tables. Its final test duration was 0.11 seconds for six source shapes.
The probe added no production behavior. The tree retains no probe code.

## Position result

Rule 12 defines each position id as local to one tier.
The task tests compare resolved file, line, and column values.
The implementation leaves each tier's position table algorithm unchanged.
Each lowering adds the suspension sites that rule 7 requires.
The interpreter retains the suspension position from the LIR terminator.
No input form lacks a required position fact.

## Red evidence

The acceptance 1–2 command was `cargo test --offline --locked -p subscript-runtime --test async_inspection`.
At `681279c5`, compilation failed with E0432 for `AsyncTaskInfo` and `subscript_rt_ctx_visit_async_tasks`.
The changed park and await arguments also produced E0061.

The acceptance 3 command was `cargo test --offline --locked -p subscript-codegen --lib async_task_snapshots_match_three_tiers`.
At `681279c5`, compilation failed with E0599 because `ReloadSession` and the interpreter had no `async_tasks` method.

## Implementation result

The runtime assigns a Context-local task id at registration and retains the current suspension position.
The Rust scheduler metadata grows from 72 to 88 bytes per registered task on this host.
The completion representation stays at 24 bytes. The existing metadata-size test now pins 88 bytes.
The visitor derives states from the scheduler's current sets, active frames, completion records, and waiter lists.
The visitor derives each awaited task id from the waiter lists.
The visitor sorts records by task id and reads function positions from live allocation headers.
A null visitor returns zero. The record has a 40-byte C layout without padding.
The C host checks all eight field offsets against the offsets that the Rust layout test pins.

Park and await calls carry the suspension position in their existing registration call.
No suspension adds a second runtime call.
Called prefixes now use `async_start` instead of a direct resume call.
This replacement lets a host function read ACTIVE for nested invocation prefixes.
The runtime keeps the frame in the existing active-frame set during that call.

`ReloadSession::async_tasks` supplies ids and resolved positions.
Each generation extends the session position table. Existing position ids keep their meaning.
The session resolves tasks and traps through that table, including code retained before a swap.
The interpreter reports resolved positions from its own coroutine state and queue.
Its weak registry retains no coroutine ownership.

The runtime tests cover all six states, both kinds, a wait ring, non-reused ids, and the C record layout.
Each state transition has a same-shape control.
The tests compare pending, unfinished, and live allocation counts before and after each read.
They also check a null visitor and direct Context and FFI reads.

The original differential test uses five programs: a completed handle, an aggregate, a wait ring, its completion control, and an async arrow.
Each program uses budget one in all three tiers.
The C host builds once per program and resolves positions through `program.alloc.h`.
The test pins the ring edges and the initial function and await positions independently from source locations.
A reload test pins old and new task positions against independent source locations.

The host header and both host tutorials cite §169 and explain blocked-task reads.
The API-reference generator reads checker and corpus data, not these header or tutorial sources.
No generated reference file requires a change.
No benchmark host, LIR text golden, or `.expected` file changes.

## Async cost

The round 6 release command was `cargo build --offline --locked --release -p subscript-benchmarks --bin async-cost -p subscript-runtime`.
Each tree used three sequential runs of `async-cost`, with its default warmup and eleven timed iterations.
The pin used isolated source storage. Both trees used the same host and C compiler.
No other benchmark or build ran during a measurement.
The acceptance comparison uses the best of the three medians for each workload.
The saved pin measurements use `681279c5`; `d8c878bd` changes only the §169 contract, so their benchmark source is identical.

| Workload | Pin medians, ns | Final tree medians, ns | Best tree / best pin |
| --- | --- | --- | --- |
| settled-awaits | 33,661,000; 33,767,000; 33,530,000 | 17,537,000; 17,454,000; 17,624,000 | 0.52055 |
| held-handles | 11,490,000; 11,423,000; 11,473,000 | 5,027,000; 4,999,000; 5,021,000 | 0.43763 |
| deep-chains | 19,730,000; 19,514,000; 19,639,000 | 12,138,000; 12,346,000; 12,308,000 | 0.62201 |

Each workload meets the 1.05 limit. The largest final spread is 11.7%, below the 20% limit.
The round 3 best ratios were 1.03612, 0.96279, and 1.00036 in the same workload order.
The added creation-position lookup first gave best medians of 39,382,000; 11,899,000; and 20,909,000 ns.
The settled-await and deep-chain ratios were 1.17453 and 1.07149, above the limit.
The async frame registry now uses the existing `AddressHasher`, as the callback registration map does.
Its keys are Context-owned frame addresses. The scheduler keeps its existing queues, ids, and task metadata.
This change removes randomized address-hash cost from registration and the added creation-position lookup.
No frame ABI or generated runtime-call count changes for this optimization.

## Validation

The focused runtime tests pass: seven tests.
The nine-program differential test passes.
The workspace all-target build passes with `--offline --locked`.
The complete compiler, codegen, and runtime package tests pass with `--offline --locked`.
`cargo fmt --check` passes.
The workspace all-target clippy check passes with `--offline --locked`.
The pin and tree each produce 71 primary warning reports, with the same messages and repository-relative files.
The comparison ignores line changes. The tree adds no warning.
The generated host-header check passes.
`tools/hygiene.sh` and `git diff --check` pass.
Every changed Rust file stays below 2,000 lines.

## Prefix trap and call position result

A call-prefix trap and a host-kick prefix trap put each invocation in the stopped set immediately.
The runtime and interpreter visitors no longer invent READY for a task outside the scheduler sets.
The runtime prepares all records before callbacks. A malformed manual registration gives no partial visit.
A direct test checks that result against a parked same-shape control.
Each path has a same-shape control without a trap in the runtime and the three-tier test.
The nine-program differential test compares stopped tasks before and after host clearance.
A stopped prefix never runs again and has no suspension position.
The existing callee-clearance test expects two stopped tasks: the child prefix and the cleared ready caller.
Its body-trap and settled-await controls still expect one stopped task.

`subscript_rt_async_start` now takes the call position as its fourth argument.
The JIT and C lowerings pass that position for direct and suspended calls.
Handle-returning callables carry a final internal call-position parameter, including the §167 producers.
Synchronous handle-returning wrappers accept that parameter and keep their own body-call positions.
The interpreter retains the resolved call position.
Host kicks and aggregates report creation position 0. Every reserved field is 0.
`ReloadSession` retains position ids across reload in its extended position table.
The differential test checks direct, suspended, and arrow creation positions against independent source locations.
The host header generator emits the 40-byte record. The host header test checks its generated output.
The tutorials state the creation position and immediate prefix STOPPED state.

## Changed files

- `codegen/src/async_inspection.rs`
- `codegen/src/cemit/async_callable.rs`
- `codegen/src/cemit/call.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/async_inspection.rs`
- `codegen/src/interpreter/collections.rs`
- `codegen/src/interpreter/inspection_tests.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/lib.rs`
- `codegen/src/lower/func.rs`
- `codegen/src/lower/func/async_callable.rs`
- `codegen/src/lower/func/call.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/src/lower/mod.rs`
- `codegen/src/reload.rs`
- `codegen/src/reload/inspection_tests.rs`
- `codegen/tests/lir.rs`
- `docs/tutorial-c-cpp.md`
- `docs/tutorial-rust.md`
- `runtime/include/subscript_runtime.h`
- `runtime/src/context.rs`
- `runtime/src/context/async_all_tests.rs`
- `runtime/src/context/async_inspection.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/tests.rs`
- `runtime/src/exception/async_completion_tests.rs`
- `runtime/src/ffi.rs`
- `runtime/src/ffi/async_frames.rs`
- `runtime/src/host_header.rs`
- `runtime/src/lib.rs`
- `runtime/tests/async_budget.rs`
- `runtime/tests/async_inspection.rs`
- `specs/tracking/s169-async-task-inspection.md`

## Session position table result

The round 5 regression command was `cargo test --offline --locked -p subscript-codegen --lib kept_ -- --nocapture`.
Both no-reload controls passed before the fix. Both reload cases failed on the round 4 implementation.
The kept async arrow resolved its function site to `kept.ts:3:13`, instead of `kept.ts:1:13`.
The kept lambda trap resolved to `kept.ts:3:59`, instead of `kept.ts:1:59`.
The arrow test also checks its suspension at `kept.ts:1:42`.
Both regression tests pass after the session table fix. The test execution took 0.07 seconds.
The complete reload unit suite passes: 31 tests, 0.13 seconds.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass on the round 5 tree.
Each new generation starts with a clone of the session table and appends its own sites.
A refused generation leaves the session table unchanged. The per-task position snapshot is removed.

## Direct await call position result

Rule 9 at `0bc658c5` defines the creation position of a direct awaited call as the enclosing `await` expression position.
The HIR and LIR carry that position. The interpreter, JIT, and C emitter retain it.
The differential test retains its independent source-position assertion for `await child()`.

## Interpreter cfg scope

The coroutine suspension-position field, its initializers, and its assignments use `cfg(test)`, as the inspection reader does.
The unused callable-position binding uses `cfg(not(test))`.

## Round 6 validation

The workspace all-target build passes with `--offline --locked`.
The complete compiler package tests pass: 1,052 passed, 1 ignored, and zero failed.
The complete codegen package tests pass: 750 passed, 1 ignored, and zero failed.
The complete runtime package tests pass: 395 passed, 1 ignored, and zero failed.
The three-tier inspection test and both kept-code reload regressions pass.
`cargo fmt --check` and workspace all-target clippy pass under the pinned toolchain.
The pin and final tree each report 71 primary clippy warnings with equal messages and repository-relative files.
The final tree adds no warning.
`tools/hygiene.sh` and `git diff --check` pass. Every changed Rust file stays below 2,000 lines.

## Phase Review and gate

The first Phase Review found two MAJOR defects: a prefix trap left a task in no state, and `subscript_rt_async_start` was outside the contract.
Rules 1, 4, and 9 changed, and round 4 closed both.
The second Phase Review closed both, and found one MAJOR defect: kept code after a reload gave ids of an old position table.
Rule 12 changed, and round 5 closed it.
The third Phase Review finds no CRITICAL or MAJOR issue.
The MINOR findings of the three reviews are §169.3 items 1 to 10.
The full gate at `0bc658c5` (the contract pin before the rule 9 wording) reports debug 2421/0/3, release 2418/0/3, goldens-moved 0, exit 0.
