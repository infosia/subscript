# §177: Cost measurement

Hardware: MacBook Air, Apple M2, eight CPU cores, 16 GB memory, arm64.
The CPU has four performance cores and four efficiency cores.
Software: macOS 27.0.1, Rust 1.95.0, Apple Clang 21.0.0.
The measurement date is 2026-10-07.
HEAD is `9767cd18`.

All builds use the release profile, `--offline`, and `--locked`.
C builds use `-O2 -ffp-contract=off -fwrapv`.
Each timed binary runs alone, with no concurrent build, test, or gate.
Each quantity gives seconds, best of three, unless the table gives a count or a ratio.
The CLI reads temporary source copies.
Separate worktrees provide each comparison pin.

## A. Interpreter preparation from checked HIR

Comparison pins: `9647c72a`, `ae165007` (§172), and `9767cd18`.
The fixed input set contains the 265 interpreter-selected entries at `9647c72a`, with that pin's source bytes and exclusion headers.
One sequential loop checks and prepares each entry.
The stage timer starts after `check_program` returns.
It includes LIR construction, every LIR verifier pass, and `Interpreter::new`.
The interpreter executes LIR directly; it has no separate instruction-lowering pass.
Its preparation computes layouts, frame lifetime values, and global storage.
These measurements execute no script.

| Fixed input set | `9647c72a` | `ae165007` | HEAD | HEAD prototype |
|---|---:|---:|---:|---:|
| LIR build, before verifier | 0.158372736 | 0.274920671 | 0.277251626 | 0.051769008 |
| `verify_module_entries`, layout and field-description checks | 0.000008082 | 0.000299836 | 0.000298322 | 0.000305540 |
| `verify_lifetime::verify` | 0.000993373 | 0.001030415 | 0.001050966 | 0.001080713 |
| `verify_structure_and_types`¹ | 0.007751040 | 0.007958647 | 0.008730977 | 0.009069895 |
| `verify_counted_stores` | 0.001027632 | 0.001125746 | 0.001131543 | 0.001201216 |
| `verify_counted_operations::verify` | 0.001109034 | 0.001703438 | 0.001825274 | 0.001902743 |
| `verify_generator_counts::verify` | — | — | 0.000133162 | 0.000144225 |
| `verify_raise_edges` | 0.002223244 | 0.002390983 | 0.002250142 | 0.002359779 |
| `verify_dominance` | 0.004196919 | 0.004443002 | 0.004447555 | 0.004651091 |
| `verify_narrowing::verify_narrowing` | 0.000148603 | 0.000154284 | 0.000180955 | 0.000183209 |
| `verify_address_invalidation` | 0.000614383 | 0.000641306 | 0.000669542 | 0.000704076 |
| `Interpreter::new` | 0.005046664 | 0.005665949 | 0.005644487 | 0.005883867 |
| Complete HIR-to-interpreter preparation | 0.182329069 | 0.301355647 | 0.304638325 | 0.080402704 |

¹ This pass includes instruction contracts, operand types, constants, and terminator types.
A dash means that the pin has no such pass.
Each row selects its own minimum; the pass minima do not necessarily sum to the total minimum.

The serial total grows by 65.3% at §172 and 67.1% at HEAD on fixed inputs.
Thus worker overlap alone does not explain the historical +23%.
The historical 2.573282/3.160941-second intervals overlap; they do not equal this serial stage total.
HIR checks and script execution stay outside the new stage timer.

Each pin's own interpreter-selected corpus gives the following additional result.

| Pin | Entries | Complete preparation |
|---|---:|---:|
| `9647c72a` | 265 | 0.185065709 |
| `ae165007` | 267 | 0.297649944 |
| `9767cd18` | 270 | 0.319053452 |

The dominant change occurs inside `FunctionBuilder::emit`, before verification.
The §172 map guard calls `intrinsic_operations()` for call instructions, including calls that cannot be Map operations.
This function formats every operation name and constructs every table row.
The caller then destroys the temporary Strings and vectors.
The older array ownership checks already construct the same table repeatedly.

| Fixed-input construction detail | `9647c72a` | `ae165007` | HEAD | HEAD prototype |
|---|---:|---:|---:|---:|
| `Lowering::new` and `Lowering::run` | 0.148781960 | 0.264450968 | 0.266805283 | 0.040837757 |
| Class field release descriptions | — | 0.000362845 | 0.000359840 | 0.000369831 |
| Unroll and suspension liveness | 0.009555251 | 0.009937669 | 0.010027921 | 0.010373550 |
| `intrinsic_operations` construction, inclusive within LIR | 0.097425837 | 0.191959579 | 0.194297588 | 0.005406747 |
| Operation table constructions | 6721 | 13055 | 13055 | 266 |

The construction timer excludes subsequent table destruction.
Class descriptions cost less than 0.0004 seconds; they do not account for the growth.

For C call checks and K operation rows, repeated table construction performs O(CK) work and allocations.
K is fixed in this compiler, so input growth is linear in emitted calls, with a large constant.
A fix constructs one immutable table and borrows it for ownership checks.
An index by family and operation also removes the linear table searches.

The prototype uses one immutable `OnceLock` table for the three builder lookup sites.
Each output module still constructs its own operation table.
Thus 265 inputs require 266 constructions, instead of 13,055.
The complete preparation falls from 0.304638325 to 0.080402704 seconds, a 73.6% reduction.
All selected inputs pass the normal verifier and interpreter preparation.
The prototype changes no verifier rule.

## B. Annotated instance field checks

Comparison pins: `40b0cbff` and HEAD.
Each generated file contains 6,000 classes and an empty exported `main(): void`.
Each class declares four `i32` instance fields.
The read initializers are `a = 1`, `b = this.a + 1`, `c = this.b + 1`, and `d = this.c + 1`.
The constant initializers are `a = 1`, `b = 2`, `c = 3`, and `d = 4`.
Every field has an explicit annotation.
The CLI check includes loading, parsing, and the provisional and final checker passes.

| Form, initial comparison | `40b0cbff` | HEAD | HEAD / pin |
|---|---:|---:|---:|
| reads | 0.193223791 | 0.203960375 | 1.056 |
| constants | 0.135288750 | 0.140927333 | 1.042 |

The excess remains, but this run does not reproduce the historical 1.16–1.17 ratios.
Temporary function timers identify the following work.
The counters sum all calls in each run and select the best of three sums.
Inclusive function intervals overlap; do not add these rows.

| Checker function or region | Pin reads | HEAD reads | Pin constants | HEAD constants |
|---|---:|---:|---:|---:|
| `run_with_effects` | 0.139646750 | 0.155861376 | 0.099732292 | 0.111799500 |
| `resolve_signatures` | 0.020725374 | 0.022170791 | 0.021712292 | 0.023513500 |
| `resolve_class_shape` | 0.015643470 | 0.016776289 | 0.016305625 | 0.017554660 |
| `check_class_body` | 0.078753637 | 0.087732896 | 0.046017169 | 0.051625767 |
| `check_class_body`: field context setup | 0.006516219 | 0.004458011 | 0.007139427 | 0.004608455 |
| `FnCtx::with_synthetic_owner` | 0.049939126 | 0.057376564 | 0.016105978 | 0.020881800 |
| `check_expr` | 0.065815269 | 0.071767710 | 0.007972996 | 0.008890612 |
| `check_member_read` | 0.011211446 | 0.013038444 | — | — |
| `require_assignable` | 0.001019977 | 0.001036078 | 0.001062696 | 0.001095089 |
| `require_field_values` | 0.006049787 | 0.006112007 | 0.006039599 | 0.006217834 |
| `decide_declarations` | — | 0.000707417 | — | 0.000748791 |
| `check_local_assignments` | — | 0.002710334 | — | 0.000821500 |
| `capture::check` | 0.003450917 | 0.004272000 | 0.001180875 | 0.002323500 |
| `narrowing::Analysis::from_module` | 0.006273291 | 0.006602125 | 0.002878000 | 0.003236583 |

`check_class_body` runs 12,000 times: once per class in each checker pass.
It sets up 48,000 field contexts and calls `with_synthetic_owner` 48,000 times.
HEAD clones an empty context snapshot for each field.
The first synthetic-owner push copies shared empty vectors and allocates new owner stacks.
The context setup itself costs less than the pin's per-field `FnCtx::new` setup.
The excess occurs after that setup, especially in `with_synthetic_owner` and expression checks.

`check_expr` runs 120,000 times for reads and 48,000 times for constants.
`check_member_read` runs 36,000 times for reads.
`check_expr_with_header_receiver` repeats apparent-type resolution and constructs a boxed `void[]` type for each comparison.
Its newer expression-work wrapper also pushes and pops one entry per expression.
`require_assignable` runs 48,000 times; its measured cost does not materially grow.

`resolve_class_shape` runs 12,000 times and constructs field types and decision states for each class.
`require_field_values` runs 12,000 times and scans the four field spellings and their checked initializers.
`decide_declarations` scans the decided declaration states in both passes.
`check_local_assignments` visits every field initializer again to find nested lambdas, even when none exists.
`capture::check` also visits all field initializers to derive capture and escape facts.
These scans explain additional cost outside the field expression checks.

A fix reuses empty synthetic-owner buffers while it preserves independent field contexts.
It also resolves the checked expression's apparent type once and tests `void[]` without a new allocation.
Shared lambda-presence facts can remove the separate no-lambda search in local assignment analysis.
The prototype does not remove that search or the two checker passes.

The first prototype swaps two empty owner buffers through each field context and retains their capacity within each class.
The second prototype adds the single apparent-type decision and the allocation-free `void[]` test.
Both preserve the existing expression checks and field-order restrictions.

| Prototype comparison | Form | Pin | HEAD | Prototype |
|---|---|---:|---:|---:|
| Owner buffers, initial batches | reads | 0.193223791 | 0.203960375 | 0.192845750 |
| Owner buffers, initial batches | constants | 0.135288750 | 0.140927333 | 0.135289875 |
| Owner buffers, alternate binary order | reads | 0.185136458 | 0.202281209 | 0.202545708 |
| Owner buffers, alternate binary order | constants | 0.131287417 | 0.139177292 | 0.136010583 |
| Owner buffers and type tests, alternate binary order | reads | 0.174482541 | 0.188430167 | 0.188989541 |
| Owner buffers and type tests, alternate binary order | constants | 0.124919709 | 0.134616750 | 0.130317625 |

The buffer prototype's initial read improvement does not survive the alternate order.
The constant improvement survives: 0.139177292 to 0.136010583 seconds for buffers alone.
The combined prototype gives 0.134616750 to 0.130317625 seconds for constants in its paired run.
That paired run gives no read improvement: 0.188430167 to 0.188989541 seconds.
Thus the prototype removes part of the constant cost, but it does not prove a fix for the read excess.
The separate scans and measured sample variation remain material.
All 15 copied field-initializer corpus checks match HEAD in exit status, standard output, and diagnostic output.

## C. Live tasks, inspection, and registration

The baseline and both prototypes use HEAD.
The program creates one shared task that suspends once, then returns 1.
It creates n tasks that await that shared handle and stores their handles in an array.
Only after all n calls does `main` await the handles and print their sum.
The prefix therefore holds n waiting tasks, the shared parked task, and the suspended main task simultaneously.
The generated values of n are 1,000, 10,000, and 100,000.
Each output equals n, and every native run reports trap kind 0.

The JIT harness compiles once before its three samples.
Each sample includes a fresh Context, initialization, all script entries, all checkpoints, and Context release.
The C host uses the same execution span after compilation.
The interpreter harness checks and constructs LIR before its three samples.
Each interpreter sample includes interpreter setup, execution, checkpoints, and release.
No program calls the visitor during the program timing.

| n | Dev JIT | C AOT | Interpreter |
|---:|---:|---:|---:|
| 1,000 | 0.000255375 | 0.000155000 | 0.005000167 |
| 10,000 | 0.002310875 | 0.001285000 | 0.112500709 |
| 100,000 | 0.034563334 | 0.016030000 | 17.078208042 |

All individual 100,000-task samples stay below 60 seconds.

The Rust visitor harness registers one parked shared frame and n waiting frames.
Its callback checks each task's state and awaited id and increments a count.
It checks n + 1 records and leaves the Context live between samples.
Frame allocation, task registration, and Context release stay outside this timer.

| n waiting tasks | Registered tasks | One HEAD visit | One prototype visit |
|---:|---:|---:|---:|
| 1,000 | 1,001 | 0.000550583 | 0.000077416 |
| 10,000 | 10,001 | 0.084842000 | 0.000790542 |
| 100,000 | 100,001 | 10.372940584 | 0.009833750 |

`visit_async_tasks` scans queues and searches all task waiter lists for each task.
For n tasks, q queued jobs, and w waiter entries, classification can cost O(n(q + n + w)).
This workload has w proportional to n, so the visit costs O(n²).
The existing task-id sort costs O(n log n).

A fix builds the ready, parked, and stopped sets and a waiter-to-awaited-id map before the task loop.
The prototype uses that form and keeps the task-id sort and state precedence.
Its total cost is O(n log n + q + w), with O(n + q + w) scratch storage.
At 100,000 waiting tasks, the visit falls from 10.372940584 to 0.009833750 seconds, a 1,054.8-fold speedup.
The seven existing async-inspection tests pass with the visitor prototype.

`register_task_id` calls HashMap `retain` before every interpreter registration.
A HashMap retain visits its capacity, including empty buckets.
With n live handles, registration performs a sum of scans over capacities proportional to 1 through n.
Thus an ordinary async program with many simultaneously held tasks pays quadratic interpreter registration cost.
It does not require a host inspection call or a contrived waiter ring.

A fix removes the per-registration scan and removes dead entries at task retirement.
A threshold-based sweep can handle residual weak entries.
The prototype retains the existing individual retirement removals and sweeps only when the new id is a power of two.
For this live-growth workload, the geometric sweep capacities sum to O(n), instead of O(n²).
This prototype permits dead weak metadata between sweeps; it does not establish a final metadata-retention bound.

| n | HEAD interpreter | Registration prototype | HEAD / prototype |
|---:|---:|---:|---:|
| 1,000 | 0.005000167 | 0.004558458 | 1.097 |
| 10,000 | 0.112500709 | 0.044987459 | 2.501 |
| 100,000 | 17.078208042 | 0.490423625 | 34.823 |

The visitor prototype also exists in the native control builds; these programs never call that API.

| n | Prototype dev JIT | Prototype C AOT |
|---:|---:|---:|
| 1,000 | 0.000268625 | 0.000151000 |
| 10,000 | 0.002564291 | 0.001459000 |
| 100,000 | 0.032789666 | 0.016442000 |

| Growth from n to 10n | 1,000 → 10,000 | 10,000 → 100,000 |
|---|---:|---:|
| Dev JIT | 9.049 | 14.957 |
| C AOT | 8.290 | 12.475 |
| HEAD interpreter | 22.499 | 151.805 |
| Registration prototype interpreter | 9.869 | 10.901 |
| HEAD visitor | 154.095 | 122.262 |
| Visitor prototype | 10.212 | 12.439 |

The registration prototype changes the larger interpreter growth factor from 151.805 to 10.901.
HashMap capacity steps and cache effects explain why the measured quadratic factors need not equal exactly 100.
The measured gain confirms the registration scan as the dominant large-n interpreter cost.

### Files and final tree

The retained file is `specs/tracking/s177-cost-measurement.md`.
All production edits exist only in the temporary pin worktrees and return to their respective HEADs.
No prototype changes a contract or lands a production rule change.

Temporary edited or added repository files, across all four pins:

- `compiler/src/lib.rs`
- `compiler/src/s177_timer.rs`
- `compiler/src/check/assignment_flow.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/initializer.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/type_rules.rs`
- `codegen/src/lib.rs`
- `codegen/src/s177_timer.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_inspection.rs`
- `codegen/src/jit.rs`
- `codegen/src/jit/bench.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/verify.rs`
- `codegen/examples/s177_a.rs`
- `codegen/examples/s177_c.rs`
- `codegen/examples/s177_corpus.rs`
- `runtime/src/context/async_inspection.rs`
- `runtime/examples/s177_visit.rs`

Temporary measurement inputs comprise two field files, three task files, copied corpora, and the C host `entry.c`.
Temporary Python drivers comprise `setup.py`, `setup_b.py`, `setup_c.py`, `measure_a.py`, `measure_b.py`, and `measure_c.py`.
The other drivers are `refine_a.py`, `refine_run_a.py`, `deeper_b.py`, `prototype_c.py`, `final_b.py`, `report.py`, and `write_note.py`.
Temporary evidence comprises build and sample logs, four prototype diffs, timer text, and JSON summaries.
Generated Rust and C binaries remain build artifacts, not source changes.

Each temporary worktree gives an empty `git status --short` after the revert.
The original tree gives only `?? specs/tracking/s177-cost-measurement.md`.
The original `git diff --stat` is empty; no tracked production change remains.
The final release CLI and runtime build use the original HEAD sources.

## Implementation

The implementation uses one immutable operation table and an index by family and operation.
Each LIR module clones its recorded rows from that table.
Builder ownership checks borrow the indexed row.
The verifier indexes the module's recorded rows separately and indexes the array semantic classes once per process.
The interpreter keeps task ids and increasing-id collection order in every build.
Only the task registry and its sweep state are inside `cfg(test)`.
Task retirement removes its registry entry.
A threshold sweep removes dead weak entries and reduces the registry capacity.
The runtime visitor builds state sets and the awaited-id map before its task loop.
It also indexes active frames, so nested active calls do not add a task-by-stack scan.

The release measurements use the same hardware and toolchain as the measurement sections above.
The implementation measurements span 2026-10-07 and 2026-10-08 in Asia/Tokyo.
Each binary runs alone, with no concurrent build, test, or gate.
Each quantity is the best of three samples.
Temporary source copies contain the timers and harnesses; the working tree contains neither.
The implementation comparison pin is `9767cd18`.

### Preparation and task growth

The preparation run uses the fixed 265-entry set and source bytes from `9647c72a`.
The timer excludes HIR checks and script execution, as section A requires.

| Preparation | Seconds | Limit |
|---|---:|---:|
| Complete HIR-to-interpreter preparation | 0.074727127 | 0.182 |

The live-task program and visitor harness use the inputs and execution spans from section C.
Each script output equals n.
Each visit reads n + 1 records and checks the parked task and each waiting task's awaited id.

| n | Interpreter seconds | Visitor seconds |
|---:|---:|---:|
| 1,000 | 0.004754209 | 0.000081208 |
| 10,000 | 0.047325041 | 0.000795959 |
| 100,000 | 0.490286792 | 0.010212459 |

The 10,000-to-100,000 interpreter growth is 10.359987; its limit is 15.
The visitor growth is 12.830383; its limit is 15.
Both 100,000-task times pass their respective 1-second and 0.05-second limits.

### Corpus and benchmark controls

The release interpreter run uses all 270 runnable entries from the pin, including `cost: benchmark`.
Every output matches its frozen golden in all six runs.
The native benchmark controls run separately under the dev JIT and C AOT.
The corpus execution span includes interpreter setup, execution, checkpoints, and release.
The complete loop also includes source reads, HIR checks, and LIR construction.

| Release interpreter corpus span | Pin seconds | Implementation seconds | Implementation / pin |
|---|---:|---:|---:|
| Execution | 206.701526375 | 209.399303923 | 1.013052 |
| Complete sequential loop | 207.118085584 | 209.614835375 | 1.012055 |

The full-corpus samples are:

| Sample | Pin execution seconds | Implementation execution seconds |
|---:|---:|---:|
| 1 | 208.115515547 | 248.697110259 |
| 2 | 206.701526375 | 209.399303923 |
| 3 | 207.915955182 | 324.466118039 |

The implementation samples have substantial variation.
The acceptance criterion uses the best of three; the tables retain all three execution samples.
No cause is assigned to the variation.

An additional non-benchmark control uses the pin's 269 entries that the debug corpus gate selects.
Every output matches its frozen golden.

| Non-benchmark interpreter corpus span | Pin seconds | Implementation seconds | Implementation / pin |
|---|---:|---:|---:|
| Execution | 1.158145962 | 1.157892751 | 0.999781 |
| Complete sequential loop | 1.570718417 | 1.344791416 | 0.856163 |

The ten cross-language workloads retain the pin's source bytes and workload parameters.
The additional `a22` row covers the performance gate's matrix workload.
JIT compilation precedes its three discarded warm-up calls and three timed calls.
C AOT compilation precedes its warm-up floor of 200 ms and three timed calls.
Both tiers time only the exported workload call.
Context creation, initialization, release, and output reads stay outside these benchmark timers.
All checksums match across the pin, the implementation, and both tiers.

| Workload | Tier | Pin seconds | Implementation seconds | Implementation / pin |
|---|---|---:|---:|---:|
| fib-recursive | Dev JIT | 0.009621959 | 0.009065125 | 0.942129 |
| fib-recursive | C AOT | 0.004236000 | 0.004236000 | 1.000000 |
| fib-loop | Dev JIT | 0.081509042 | 0.079267625 | 0.972501 |
| fib-loop | C AOT | 0.033929000 | 0.033242000 | 0.979752 |
| mandelbrot | Dev JIT | 0.146306375 | 0.147929208 | 1.011092 |
| mandelbrot | C AOT | 0.137900000 | 0.139094000 | 1.008658 |
| primes | Dev JIT | 0.035424334 | 0.035507250 | 1.002341 |
| primes | C AOT | 0.023379000 | 0.023493000 | 1.004876 |
| sort | Dev JIT | 0.038533167 | 0.039250167 | 1.018607 |
| sort | C AOT | 0.020270000 | 0.019554000 | 0.964677 |
| tree | Dev JIT | 0.463150916 | 0.457207333 | 0.987167 |
| tree | C AOT | 0.120610000 | 0.115646000 | 0.958843 |
| queen | Dev JIT | 0.040729250 | 0.039280417 | 0.964428 |
| queen | C AOT | 0.029246000 | 0.027887000 | 0.953532 |
| particles | Dev JIT | 0.517910125 | 0.511820375 | 0.988242 |
| particles | C AOT | 0.088217000 | 0.086286000 | 0.978111 |
| callbacks | Dev JIT | 0.308244291 | 0.297729208 | 0.965887 |
| callbacks | C AOT | 0.041727000 | 0.040348000 | 0.966952 |
| collect | Dev JIT | 0.125479000 | 0.122277916 | 0.974489 |
| collect | C AOT | 0.038335000 | 0.038691000 | 1.009287 |
| a22-matrix-propagation | Dev JIT | 0.092780583 | 0.091135375 | 0.982268 |
| a22-matrix-propagation | C AOT | 0.006267000 | 0.006082000 | 0.970480 |

Every final benchmark ratio is below 1.05.
The first C AOT primes comparison gave 0.023443 versus 0.024668 seconds, a ratio of 1.052255.
A fresh paired batch gave 0.023379 versus 0.023493 seconds, a ratio of 1.004876, as the table records.
Each batch uses three timed samples; the initial result remains visible here.

### New gate test cost

Each debug test runs alone in its compiled test binary, with one test thread.
Each cost includes process start, test-harness setup, the test, and process release; compilation stays outside it.
Each row is the best of three process times.
The 100,000-task measurements remain outside the gate tests.

| Test | State and control | Seconds |
|---|---|---:|
| `lir::operation_table::tests::many_calls_share_one_table_across_modules` | Two independently lowered modules, each with 256 calls; one row copy, then two | 0.026816625 |
| `interpreter::inspection_tests::registry_retirement_and_live_control_keep_bounded_metadata` | 256 registrations with retirement, against 256 live holders; dead weak entries against live weak entries | 0.008912209 |
| `one_visit_reads_all_states_with_a_complete_control` | Six invocation states in one visit, against the same frame shape with completion instead of a trap | 0.003427459 |
| `collected_tasks_trap_in_task_id_order` | Five failed reference-field tasks; collection reports the first task and no stdout | 0.007761500 |

The row-copy counter is test-only and local to the test thread.
A temporary defect control copies the rows for each call instruction.
The test fails with 257 copies instead of one; the unmodified implementation reads one copy, then two.

`t107-collected-task-order` is Red against the round 1 non-test interpreter under `cargo test --release`.
The measured trap is `uncaught-exception` at 23:3, with `Error: third` instead of `Error: first` and no stdout.
At `dc3acdb4`, the dev JIT, C AOT, and interpreter all report `Error: first` at 23:3 with no stdout.
The fixed debug and release interpreter tests report the same result.
TypeScript 5.9.2 accepts the entry.
The source is copied to temporary storage before each CLI run.

The operation-table submodule keeps `lir.rs` below the 2,000-line limit with its added test.
The interpreter source has 1,986 lines.
Only `t107` adds a golden; no existing golden or compiler contract changes.
