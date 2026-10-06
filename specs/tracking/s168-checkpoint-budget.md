# §168: A checkpoint with a dispatch budget

## Contract pin

The pin is `30100fc18979472988ac52db6455df2880ab647c`.

## Red evidence

The runtime acceptance tests fail to compile at the pin.
The compiler reports E0432 for `AsyncStepReport` and `subscript_rt_ctx_async_step_budget`.
It reports E0599 for `Context::async_step_budget`.
These tests cover completed chains, queue order, zero budgets, traps, collection, and the report layout.

The differential acceptance test fails to compile at the pin.
The pin has no budgeted step in `ReloadSession` or the interpreter.
The C host test uses the new API and checks each C field offset.

The C layout test fails to compile against the pin header.
The C compiler reports an unknown report type and an undeclared budgeted step.

## Implementation evidence

Each tier uses one drain for bounded and unbounded checkpoints.
The runtime uses tagged invocation and aggregate jobs.
The trap record keeps the job kind until host clearance.
Host clearance removes a trapped aggregate reaction without a second reaction.

The report has four `u64` fields and the `repr(C)` attribute.
The Rust and C tests check size 32 and field offsets 0, 8, 16, and 24.
A zero budget preserves the parked list and runs no script.
If ready work exists at a zero budget, `budget_exhausted` is 1, as rule 7 states.

The differential test checks exact reports and output at budgets 1, 3, and `u64::MAX`.
It also checks the zero-budget report before each run.
The C host explicitly collects between checkpoints.
The runtime tests compare two completed chains with an unbounded control.
The trap tests check no replay after host clearance.

The host header and both host tutorials cite §168 and state rule 6.
The host-header generator changes `runtime/include/subscript_runtime.h`.
The API reference generator rewrites its three outputs without a content change.
No generated document changes. No expected golden changes.

## Cost evidence

Host: aarch64 macOS. Profile: release. Pin sources and build outputs use separate scratch storage.
Each revision runs three times. Each run uses at least three warmups and an execution floor of 200 ms.
Each run then uses eleven timed iterations. The comparison uses the lowest median of the three runs.
The timed span includes Context creation, initialization, exports, all checkpoints, and Context release.
It excludes code compilation. No other build or benchmark runs during a measurement.

All values below use milliseconds.

| Workload | Pin medians | Tree medians | Pin best | Tree best | Ratio |
| --- | --- | --- | ---: | ---: | ---: |
| settled-awaits | 33.208, 32.821, 32.607 | 33.227, 33.088, 32.494 | 32.607 | 32.494 | 0.997 |
| held-handles | 11.302, 11.128, 11.236 | 10.986, 11.216, 11.032 | 11.128 | 10.986 | 0.987 |
| deep-chains | 19.584, 18.833, 19.028 | 18.989, 18.965, 18.745 | 18.833 | 18.745 | 0.995 |

Every ratio is below 1.05. Every run has a spread below 20%.
Each extra settled-awaits run uses a budget of 1,000 and takes 200 calls.
The maximum call times are 0.200, 0.183, and 0.172 ms.
The extra run follows the existing timed samples and checks the same output.
These measured call times do not establish a time limit for one dispatch.

## Validation evidence

| Command | Result |
| --- | --- |
| `cargo build --offline --locked --workspace --all-targets` | Pass |
| `cargo test --offline --locked -p subscript-compiler` | 1,052 pass; one existing ignored test |
| `cargo test --offline --locked -p subscript-codegen` | 746 pass; one existing ignored test |
| `cargo test --offline --locked -p subscript-runtime` | 388 pass; one existing ignored test |
| `cargo fmt --check` | Pass |
| `cargo clippy --offline --locked --workspace --all-targets` | Pass; no new warning |
| `tools/hygiene.sh` | Pass |

The codegen unit suite runs 240 tests in 3.07 seconds, including both new budget tests.
Clippy reports 18 runtime library warnings and 13 codegen library warnings at existing sites.
The only warning in a changed file names the unchanged `callbacks: Vec<Box<CallbackBinding>>` field.
Every changed Rust file stays below 2,000 lines.
The tree keeps the contract pin as HEAD. The tree contains no new commit.

## Changed files

- `benchmarks/async-cost-entry.c`
- `benchmarks/src/bin/async-cost.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/budget_tests.rs`
- `codegen/src/interpreter/checkpoint.rs`
- `codegen/src/reload.rs`
- `docs/tutorial-c-cpp.md`
- `docs/tutorial-rust.md`
- `runtime/include/subscript_runtime.h`
- `runtime/src/context.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/ffi.rs`
- `runtime/src/host_header.rs`
- `runtime/src/lib.rs`
- `runtime/tests/async_budget.rs`
- `specs/tracking/s168-checkpoint-budget.md`

## Phase Review and gate

The Phase Review finds no CRITICAL or MAJOR issue.
Its MINOR findings are §168.3 items 1 to 4, and the §94 rule 12 text, which the contract commit holds.
It finds that the change to aggregate trap clearance closes a §94.2 defect: the runtime consumed a trapped aggregate reaction, so `pending` after the trap was one lower than in the interpreter.
The full gate at `30100fc1` (the contract pin before the §94 rule 12 text) reports debug 2410/0/3, release 2407/0/3, goldens-moved 0, exit 0.
