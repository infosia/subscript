# §166: Promise.all over an array of handles

## Contract-pin evidence

The initial contract pin is `018757750e72d20d5875149ce6460fd3146ad0fd`.
The round 2 contract pin is `2b45b0ee`.
The round 3 contract pin is `d6311e53`.
All three pins produce the results below.
The TypeScript measurement uses version 5.9.2, the repository config, the prelude, and one isolated entry.

| Entry | TypeScript | Pin check | Pin run |
| --- | --- | --- | --- |
| `a335-promise-all` | Accepts, exit 0 | Exit 1: six S016, eleven S013, one S100 | Exit 1: the same diagnostics |
| `r377-promise-all-void-value` | Accepts, exit 0 | Exit 1: S016 at 11:20, S013 at 10:34 | Exit 1: the same diagnostics |
| `r378-promise-all-non-handle-array` | Accepts, exit 0 | Exit 1: S016 at 10:9 | Exit 1: the same diagnostic |
| `t83-unobserved-aggregate-exception` | Accepts, exit 0 | Exit 1: S013 at 17:45 and 16:33 | Exit 1: the same diagnostics |

The pin reports `unknown name Promise` for direct awaits of `Promise.all`.
It reports `Promise static Promise.all(...) is not in the language` for ordinary call expressions.
It also rejects input handles that only the aggregate observes.
It rejects the `void[]` annotation in `a335` with S100.
The new reject entries do not yet reach their required rejection sites.

The offline locked CLI build passes at each pin.
Node v24.18.0 produces the `a335` golden byte for byte at the current pin.

Node v24.18.0 runs the TypeScript-emitted CommonJS entry with `print` bound to `console.log`.
Node exits 0 and supplies `a335-promise-all.expected`.
The output places `success 10 20` between `turn 3` and `turn 4`.
It places `caught completion-first` before `after catch`, then `fail input-first`.
Thus, the later exception occurs after the caller catches the aggregate exception.
The entry also covers empty inputs, duplicate inputs, array changes, completed inputs, another await, and both void aggregate forms.

The `t83` expected output follows §166 and §116.
The aggregate job reads the input exception before the caller releases the holder.
The input exception is observed, but the aggregate exception remains unobserved.
The required output is `release aggregate`, followed by trap 29 with `Error: aggregate dropped`.
The pin rejects the source before it can reach that trap.

## Tagged waiter design

Round 1 stops at the frame-only waiter form; §166 rule 14 supplies the required tagged form.

The contract defines one waiter and ready-job form with two kinds.
An invocation continuation carries its frame.
An aggregate reaction carries its aggregate identity and input index.
The aggregate state holds the snapshot inputs and their counts, partial results, and completion state.
Each async registry entry identifies an invocation or an aggregate.
`async_unfinished` counts only incomplete invocations.

An input keeps these waiters in registration order.
Its completion appends them to the existing ready queue in that order.
A complete input appends its reaction when the aggregate registers it.
A checkpoint dispatches each job by its kind.
Invocation dispatch preserves the frame resume protocol.
Aggregate dispatch reads one input completion, observes its exception, stores its result, and releases its input count.
Both kinds use the same FIFO queue and §94 checkpoint boundary.

The interpreter uses the same two kinds in its own waiter lists and ready queue.
An aggregate reaction needs no interpreter `Frame` or LIR function.
Collection roots must include the aggregate state, unread inputs, partial results, and cached completion.
The runtime form serves both generated tiers; the differential gate compares them with the interpreter.
This section states the contract design; the round 1 and round 2 trees do not implement it.

## Counted-result store limit

Round 2 stopped at the untyped result store; rule 12 now excludes an async handle or an array of async handles as `T`.

## Round 1 and round 2 verification

| Check | Result |
| --- | --- |
| Offline locked workspace build, all targets | Pass |
| §154 total test | Pass: 1,602 witnesses, 467 variants |
| §154 TypeScript cost | 0.652 seconds |
| §154 checker cost | 0.638 seconds |
| `cargo fmt --check` | Pass |
| `git diff --check` | Pass |
| `tools/hygiene.sh` | Pass |

`generated-docs/corpus-index.md` contains one new row for each new entry.
`generated-docs/api-reference.md` and `generated-docs/language-reference.md` match the pin byte for byte.

## Collision references

C8 must cite `a335`, `r378`, and `t83` for the admitted combinator and its boundaries.
C24 row 34 must cite `a335` and `r377` for the void aggregate boundary.
`collisions.md` remains unchanged.

## Round 3 implementation

The runtime and interpreter use tagged invocation continuations and aggregate reactions in one ready queue.
The async registry identifies each invocation and aggregate.
The unfinished observer counts invocations only.
The aggregate holds a snapshot count for each input, including duplicates.
An input reaction observes its completion and releases its snapshot count.
A reaction stores a non-counted result by a byte copy.
The first exception completes the aggregate; later reactions still observe their input exceptions.
A released aggregate keeps its partial state until every reaction ends.
A separate report flag prevents another trap report after host clearance.
Collection roots include unread inputs, partial results, and cached completions.
The lowering releases a fresh temporary input array after the snapshot call.
A direct aggregate await transfers its temporary handle count to the await registration.
The LIR suspension carries that transfer flag from the existing fresh-owner fact.
A stored handle await retains its own registration count.
The interpreter, JIT, and C emitter use the same transfer fact.
The temporary-input test checks zero live allocations after explicit collection, with a stored-array control.

The checker accepts `Promise.all` over `Promise<T>[]` and preserves the aggregate's own must-await obligation.
The checker rejects a `void[]` value, explicit type arguments, other inputs, and counted results.
The C24 row 35 test compares an array of async handles with a non-counted `f64[]` result.
Each new rejection site has a measured TypeScript witness and a §154 table row.

The generated-code C ABI adds `subscript_rt_async_all(ctx, jobs, elem_size, pos_id)` for the snapshot call.
The C emitter derives its declaration from the runtime call; the embedding host header needs no new declaration.
The async metadata grows from 64 to 72 bytes; aggregate state uses a separate allocation.
The Context source splits into modules to keep each changed Rust file below 2,000 lines.

## Round 3 async cost

The measurement uses release builds on aarch64/macOS, three driver runs, and eleven timed samples per workload per run.
Each driver run uses at least three warm-up iterations and 200 milliseconds of warm-up execution.
Each selected median is the lowest of the three driver medians.
The pin is `d6311e53`.
Each revision uses its own runtime archive.
No other benchmark or build runs during the timed measurements.

| Workload | Pin medians (ns) | Tree medians (ns) | Selected pin (ns) | Selected tree (ns) | Tree / pin |
| --- | --- | --- | --- | --- | --- |
| `settled-awaits` | 32165000, 34117000, 32693000 | 31226000, 31888000, 32459000 | 32,165,000 | 31,226,000 | 0.9708 |
| `held-handles` | 10785000, 10934000, 10827000 | 10603000, 10979000, 11030000 | 10,785,000 | 10,603,000 | 0.9831 |
| `deep-chains` | 18233000, 18497000, 18619000 | 18484000, 18194000, 18616000 | 18,233,000 | 18,194,000 | 0.9979 |

Each selected ratio is below 1.05. Each driver run stays below the 20% noise limit.

## Round 3 verification

| Check | Result |
| --- | --- |
| Offline locked workspace build, all targets | Pass |
| Full compiler tests, including the §154 total test and TypeScript corpus | Pass |
| Full codegen tests, including interpreter, dev JIT, ship C AOT, and LIR goldens | Pass |
| Full runtime tests | Pass; one existing ignored test |
| `cargo fmt --check` | Pass |
| Offline locked workspace Clippy, all targets | Pass; no new warnings |
| Changed Rust file size | Each file has at most 2,000 lines |
| `git diff --check` | Pass |
| `tools/hygiene.sh` | Pass |

The §154 inventory has 1,608 witnesses and 471 variants.
The Node v24.18.0 output still matches the `a335` golden byte for byte.
The generator changes `generated-docs/corpus-index.md` and `generated-docs/language-reference.md`.
The generator leaves `generated-docs/api-reference.md` unchanged.
The LIR capture adds `a335` and records the explicit await ownership flag.
The independent Phase Review finds no CRITICAL or MAJOR issue.
Its MINOR finding identifies missing ownership files in this note's manifest; the manifest now lists them.

## Round 3 changed files

- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/async_all.rs`
- `codegen/src/interpreter/instruction.rs`
- `codegen/src/interpreter/tests.rs`
- `codegen/src/jit/symbols.rs`
- `codegen/src/lir/expr.rs`
- `codegen/src/lir/lambda.rs`
- `codegen/src/lir/verify_instruction.rs`
- `codegen/src/lir/verify_lifetime.rs`
- `codegen/src/lir/verify_narrowing.rs`
- `codegen/src/lir/verify_terminator.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/src/lower/mod.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir/snapshot.rs`
- `codegen/tests/promise_all.rs`
- `codegen/tests/support/lir_facts.rs`
- `codegen/tests/support/lir_facts/boundary.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/promise_all.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/diag.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/promise_all.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/effects.rs`
- `compiler/src/hir/expression.rs`
- `compiler/src/hir/shared.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/lir.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/promise_all.rs`
- `corpus/accept/a335-promise-all.expected`
- `corpus/accept/a335-promise-all.ts`
- `corpus/reject/r377-promise-all-void-value.ts`
- `corpus/reject/r378-promise-all-non-handle-array.ts`
- `corpus/trap/t83-unobserved-aggregate-exception.expected`
- `corpus/trap/t83-unobserved-aggregate-exception.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `runtime/src/context.rs`
- `runtime/src/context/async_all_tests.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/tests.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/context/tests_1.rs`
- `runtime/src/exception/async_completion_tests.rs`
- `runtime/src/ffi/async_frames.rs`
- `specs/tracking/s166-promise-all.md`

## Phase Review and gate

The Phase Review found no CRITICAL or MAJOR finding; its MINOR findings are §166.3 items 1 to 4, and the stale `collisions.md` texts that this commit corrects.
It compared the three tiers with `node` byte for byte on each order probe, and found no count residue after success, failure, collection, teardown, or reload.
The full gate at `d6311e53` reports debug 2388/0/3, release 2385/0/3, exit 0.
Its one moved golden is the LIR text golden: the `a335` functions, and the new `owned` field of each existing async await entry.
