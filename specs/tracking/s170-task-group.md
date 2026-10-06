# §170: A task group joins its tasks

## Result and form

A TaskGroup has one lexical owner and no count.
A const local creates the group. A synchronous parameter borrows the group.
The checker rejects every other holder with S009 and an unjoined declaration with S013.
It rejects a group in every position of a generator body.
A dropped iterator has no scope exit that can release the group.
The runtime stores the group outside the task registry.
A group stores its closed state, input tasks, finished and failed counts, first exception, and join handle.
Each add transfers one task count and registers a tagged group reaction immediately.
Each reaction observes the completion and releases that count.
The group records the first exception in reaction order and waits for all inputs.
The join call creates a separate counted void task of kind 3.
The task reports WAITING until completion. Its source positions name the join call.
The declaring scope releases the group on normal and exceptional exits.
An unjoined exit reports unfinished and failed task counts with trap kind 33, `TaskGroup`.
A joined exit leaves the completion obligation with the join handle.
The group roots its input tasks, first exception, and join handle during collection.
The interpreter uses the same group state and tagged reactions as both native tiers.
The invocation metadata stays at 88 bytes. The group does not enlarge this record.

The group form carries the ownership and reaction facts that its consumers need.
The completion and array count defects from rules 14 and 15 remain outside this section.
The four count measurements retain their same-shape controls and their measured defects:

- `codegen/tests/task_group_count_form.rs`: a counted async array result leaves one completed task; the synchronous control leaves none.
- `runtime/tests/counted_completion_form.rs`: a completion byte cache leaves one payload count; the direct transfer control frees the payload.
- `codegen/tests/task_group_array_count_form.rs`: removal through an array alias leaves one completed task; the direct removal control leaves none.
- `runtime/tests/task_group_array_count_form.rs`: an alias leaves one payload count; the control frees the payload.

## Pin and TypeScript evidence

The contract pin is `bbaba6e916494d5d6ca245d4e8605d00c3ad7ca5`.
The release CLI builds from a clean archive of that pin.
Each entry runs from a copy in `$TMPDIR`.
Each TypeScript configuration extends `tsconfig.json` and includes the entry and `prelude/**/*.d.ts`.
The rule 1 declaration is present before the TypeScript check.
TypeScript reports version 5.9.2.

| Entry | TypeScript exit | Pin CLI exit | First pin diagnostic |
|---|---:|---:|---|
| `a337-task-group` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `r383-unjoined-task-group` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `r384-async-task-group-result` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `r385-task-group-field` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `r386-task-group-generator-body` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `t84-dropped-unfinished-task-group` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `t85-dropped-failed-task-group` | 0 | 1 | S016: unknown type name `TaskGroup` |
| `t86-add-to-closed-task-group` | 0 | 1 | S016: unknown type name `TaskGroup` |

The command is `target/release/subscript run ENTRY`.
The prior Red evidence at `8e9c828d` and `fb42f3f3` reports the same unknown-type failure.
The amended pin `bbaba6e9` reports that failure for all eight entries.
The `r386` TypeScript witness uses stock TypeScript 5.9.2 and exits 0.
Before the generator check, the checker accepts `r386`; the new rejection test fails.
Its same-shape async body control accepts.
After the check, the generator body reports S009 at line 13.

## Hand-derived corpus results

These derivations apply rules 4 to 7 and the §94 ready queue order.
The golden files express these results independently of the implementation.

- `a337`, success: the one-turn task prints `done success-a` before the two-turn task prints `done success-b`.
  Each completion queues its group reaction. The last reaction completes the join before main prints `success joined`.
  Collection preserves the group, tasks, and join across the checkpoints.
- `a337`, failures: `completion-first` finishes at its first checkpoint; `input-first` finishes at its third checkpoint.
  Their reactions record `completion-first` first. The join waits for both reactions.
  Both task lines precede `caught completion-first` and `all failures joined`.
- `a337`, helper: the synchronous parameter borrows the local group and adds `helper`.
  The helper exit does not release the group. Its task prints `done helper` before `helper joined`.
- `a337`, immediate: the task prints `done immediate` and completes before add.
  Add queues a reaction immediately. Join waits for that reaction before main prints `immediate joined`.
- `a337`, completed: the first checkpoint completes the task and queues its reaction.
  Two explicit suspensions let that reaction finish before join. Join completes at its call.
  Its await still suspends main before `completed joined`.
- `a337`, completed failures: the one-turn task prints and fails before the two-turn task.
  Their reactions record `late-completion-first` before `late-input-first`.
  Three explicit suspensions finish both reactions. Collection preserves the first exception.
  The late join completes with `late-completion-first`. Both task lines precede its catch and `completed failures joined`.
- `a337`, empty: join completes at its call. Its await queues main before `empty joined`.
- `t84`: the task parks at its call. The false branch skips join.
  The scope prints `release unfinished group`, then traps with one unfinished task and zero failed tasks.
  The conditional join meets the static origin rule. The actual scope exit triggers the dynamic check.
- `t85`: the task fails before add. Add queues a reaction; the explicit suspension lets that reaction record the failure.
  The false branch skips join. The scope prints `release failed group`, then traps with zero unfinished tasks and one failed task.
- `t86`: the empty join completes and its await resumes main.
  Main prints `add to closed group`. The next add traps; `unreached` never prints.
- `r383`: the declaration has no join in its scope. Rule 3 requires S013 at line 10.
- `r384`: the async fulfilled type contains TaskGroup. Rule 2 requires S009 at line 9.
- `r385`: a field holds TaskGroup. Rule 2 requires S009 at line 10.
- `r386`: the generator adds a failed task, yields, and loses its iterator after the first value.
  The later join is unreachable. Rule 2 requires S009 at line 13.

C8 must cite `a337`, `r383`, `r384`, `r385`, `r386`, `t84`, `t85`, and `t86`.
C24 row 37 must cite `r384`, `r385`, and `r386`.
The collision file does not change.

## Direct checks

The group tests check add-time registration, completion before add, reaction order, all-input completion, and collection roots.
They check success and failure counts, join lifetime, lexical release, exception exits, and dropped joins with late failures.
Each runtime rule has a same-shape success, open-group, joined-scope, or observed-completion control.
After the first reaction deletes its input, collection preserves the first exception object before the join completes.
The second exception object has the same task shape; collection frees it because the group does not root it.
The join raises the same first object. After release and catch, collection frees that object.
The closed-add test checks that a failed input cannot replace the TaskGroup trap during argument release.
The task inspection test checks kind 3, WAITING, the join source position, and the waiting invocation's task id.
All remaining task records disappear after the joined success and failure controls.
The compiler tests check forbidden holders, aliases, copies, captures, member values, assignments, and asynchronous signatures.
The synchronous method and arrow controls accept borrowed parameters.
A borrow alone does not remove the local join obligation.

The §154 inventory includes TaskGroupPosition, TaskGroupGeneratorBody, TaskGroupUnjoined, and TaskGroupUnsupported.
The position and unjoined witnesses have TypeScript-clean programs and C24/C8 reasons.
The unknown-method witness records TS2339 and S009.
TypeScript exits are 0, 0, 0, and 2 for the position, generator-body, unjoined, and unknown-method witnesses.
The exhaustive site and witness checks pass.
The generic TypeScript matrix includes add and join. Its synchronous join templates return the join handle.
The matrix lists the constructor omission because its templates use call expressions.
The type matrix lists TaskGroup as unavailable in generic constraints and type arguments.
The new callable positions produce 8,937 omitted concrete instances; each omission retains its diagnostic evidence.

## Documentation and generated files

The tutorial adds a cooperative cancellation token/source example and a TaskGroup example, each with its output.
The cancellation catch tests the message against `cancelled`, prints `work cancelled`, and rethrows other exceptions.
The documentation test checks 22 program fences.
Q34 describes the lexical group, reactions, join task, lifetime, and all eight corpus ids.
`cargo run --offline -p subscript-compiler --bin generate-api-reference` generates the documentation.
The changed generated files are `generated-docs/language-reference.md` and `generated-docs/corpus-index.md`.
The generated API reference stays byte-identical.
The LIR capture adds the new accept entry to `codegen/tests/lir-goldens/corpus.txt`.
No existing `.expected` golden changes.

The host header generator adds only trap kind 33 to the public trap list.
The regenerated `runtime/include/subscript_runtime.h` gives hosts the same trap number as Rust.
The header byte comparison passes.

## Release async cost

Both release builds use `cargo build --offline --locked --release -p subscript-runtime -p subscript-benchmarks --bin async-cost`.
Each revision runs three times without concurrent builds or tests.
Each run uses at least three warm-up iterations, a 200 ms warm-up floor, and eleven timed samples.
The comparison uses the smallest median from the three runs.
The pin run links its archived runtime library. The tree run links its tree runtime library.

| Workload | Pin medians (ns) | Tree medians (ns) | Best tree / best pin | Limit |
|---|---|---|---:|---:|
| `settled-awaits` | 14869000, 14980000, 15014000 | 15009000, 15229000, 15420000 | 1.0094 | 1.05 |
| `held-handles` | 4279000, 4332000, 4319000 | 4318000, 4379000, 4316000 | 1.0086 | 1.05 |
| `deep-chains` | 10232000, 10181000, 10536000 | 10490000, 10636000, 10630000 | 1.0304 | 1.05 |

All six runs have stable output and zero unfinished tasks.
All three workloads meet acceptance 4.
The generator fix changes checker code and runtime tests. It changes no runtime hot path; these cost results still apply.

## Required checks

- `cargo build --offline --locked --workspace --all-targets`: pass.
- `cargo test --offline --locked -p subscript-compiler`: 1,056 passed, one existing ignored test.
- `cargo test --offline --locked -p subscript-codegen`: 757 passed, one existing ignored test.
- `cargo test --offline --locked -p subscript-runtime`: 403 passed, one existing ignored test.
- The four compiler group tests and the five codegen group integration tests pass.
- `cargo fmt --check`: pass.
- `cargo clippy --offline --locked --workspace --all-targets`: pass, with 111 warnings on both the pin and the tree.
- The warning comparison uses diagnostic code, message, source file, and occurrence count. It finds no added warnings.
- The pin warning baseline uses `5e4ea037`. Its Rust sources match `bbaba6e9`; only the §170 contract text differs.
- `git diff --check`: pass.
- Every changed Rust file has at most 2,000 lines.
- `tools/hygiene.sh`: pass.

The checks run offline with the locked dependency set.

## Generator restriction files

- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/task_group.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/language_reference.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/task_group.rs`
- `corpus/reject/r386-task-group-generator-body.ts`
- `docs/tutorial-typescript.md`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `runtime/src/context/task_group_tests.rs`
- `specs/tracking/s170-task-group.md`

## Changed files

- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/async_inspection.rs`
- `codegen/src/interpreter/checkpoint.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/memory.rs`
- `codegen/src/interpreter/operations.rs`
- `codegen/src/interpreter/task_group.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/exception.rs`
- `codegen/src/lir/expr.rs`
- `codegen/src/lir/verify.rs`
- `codegen/src/lir/verify_instruction.rs`
- `codegen/src/lir/verify_lifetime.rs`
- `codegen/src/lir/verify_narrowing.rs`
- `codegen/src/lir_types.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/src/lower/mod.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/docs.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/support/lir_facts.rs`
- `codegen/tests/support/lir_facts/boundary.rs`
- `codegen/tests/task_group.rs`
- `codegen/tests/task_group_array_count_form.rs`
- `codegen/tests/task_group_count_form.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/task_group.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/diag.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/effects.rs`
- `compiler/src/hir/expression.rs`
- `compiler/src/hir/shared.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lir.rs`
- `compiler/src/types.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/task_group.rs`
- `corpus/accept/a337-task-group.expected`
- `corpus/accept/a337-task-group.ts`
- `corpus/reject/r383-unjoined-task-group.ts`
- `corpus/reject/r384-async-task-group-result.ts`
- `corpus/reject/r385-task-group-field.ts`
- `corpus/reject/r386-task-group-generator-body.ts`
- `corpus/trap/t84-dropped-unfinished-task-group.expected`
- `corpus/trap/t84-dropped-unfinished-task-group.ts`
- `corpus/trap/t85-dropped-failed-task-group.expected`
- `corpus/trap/t85-dropped-failed-task-group.ts`
- `corpus/trap/t86-add-to-closed-task-group.expected`
- `corpus/trap/t86-add-to-closed-task-group.ts`
- `docs/tutorial-typescript.md`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `prelude/lang.d.ts`
- `runtime/include/subscript_runtime.h`
- `runtime/src/context.rs`
- `runtime/src/context/async_inspection.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/task_group.rs`
- `runtime/src/context/task_group_tests.rs`
- `runtime/src/ffi/async_frames.rs`
- `runtime/src/host_header.rs`
- `runtime/src/trap.rs`
- `runtime/tests/counted_completion_form.rs`
- `runtime/tests/task_group_array_count_form.rs`
- `specs/tracking/s170-task-group.md`

## Phase Review and gate

The first Phase Review found one MAJOR defect: a group local in a generator body ended with no scope exit when the iterator was dropped.
Rules 2 and 7 changed, and round 5 closed it with `r386`.
The second Phase Review finds no CRITICAL or MAJOR issue; it measured each frame that can end with no scope exit.
The MINOR findings are §170.3 items 1 to 4; item 5 records the count defects that rule 15 names.
The full gate at `bbaba6e9` (the contract pin before the rule 7 wording) reports debug 2440/0/3, release 2437/0/3, exit 0.
Its one moved golden is the LIR text golden: the `a337` functions.
