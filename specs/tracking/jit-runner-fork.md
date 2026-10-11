# The dev-JIT runner forks a multithreaded process

Status: **measurement, 2026-10-11. Contract: §190.** Measured at `2867fd14` on
`aarch64-apple-darwin`, 8 logical CPUs, toolchain 1.95.0. Each
prototype below was reverted. This note changes no contract.

## Fact

`execute_entry_retained` (`codegen/src/jit/entry.rs`) calls `fork` and
runs the program in the child without `exec`. The child runs Rust code:
it allocates, starts Worker threads (`runtime/src/worker.rs`,
`std::thread::Builder::spawn`), and can panic or print to stderr. After
`fork` in a multithreaded process, POSIX allows only async-signal-safe
calls in the child *(docs)*. A lock that another parent thread held at
the `fork` stays locked in the child, and no thread in the child can
release it.

A std thread start takes a process-wide mutex in
`std::sys::pal::unix::stack_overflow::thread_info::set_current_info`
(from `make_handler`). If a parent thread starts at the moment of the
`fork`, the child inherits that mutex as locked. The child's first
Worker start then waits forever, and the child's main thread waits in
`subscript_rt_worker_join`. The parent waits in `waitpid` for that child.

## Reproduction

| Setup | Runs | Wall | Hangs |
|---|---:|---:|---:|
| release `exceptions` binary, default test threads, 8 busy-loop processes as CPU load | 120 | 363 s | 0 |
| release stress test: one Worker program through `run_jit`, 6 other threads that start and join empty threads without pause | 6 trials | — | 6 of 6, at runs 48, 9, 71, 20, 4, 31 |
| the same stress test, 0 other threads (control) | 3,000 | 6.8 s | 0 |

The stress rate is 6 hangs in 189 runs, about 1 in 32. A stack sample
of one hung stress child matched the gate sample: the main thread in
`Worker::join` → `pthread_join`, and the Worker thread in `make_handler`
→ `set_current_info` → `__psynch_mutexwait`.

In the gate, the rate is low but not zero. One release step of the 14
full-gate records on disk (`20261010T135620Z`) stopped at the 3,600 s
bound in `a_worker_exception_stays_the_worker_trap_under_an_await`. The
other 27 suite steps in those records did not hang. Under libtest, the
parent starts one thread for each test, so a thread start can coincide
with a `fork` in any binary that runs tests in parallel.

## Fork sites

`codegen/src/jit/entry.rs` `execute_entry_retained` is the only `fork`
in the repository. No other file calls `fork`, `vfork`, `pre_exec`, or
`daemon`. Every `run_jit*` entry point in `codegen/src/jit/run.rs`
reaches it, except a run with `memory_accounting` (in process) and the
benchmark path `run_entry` (in process).

In the forked child, these operations can wait on a lock that a parent
thread held at the `fork`:

| Operation in the child | Lock | Reached by |
|---|---|---|
| Worker start | std `thread_info` mutex | `Worker.spawn` (observed) |
| panic message, `eprintln!` | std stderr lock, panic hook lock | a foreign call that panics; `SUBSCRIPT_MARK_TRACE` output |
| `std::env::var` | std environment lock | `mark_trace_target` at each collection |

A change that removes one row leaves the others. Only a child that does
not run Rust code after `fork`, or a run with no `fork`, closes the class.

The CLI `subscript run` (`cli/src/lib.rs`) also reaches the `fork`. The
CLI starts no thread before the run (read from the source, not
measured), so its parent is single-threaded at the `fork`.

## Options

Each option was a prototype behind an environment switch in
`run_jit_configured`. Costs are the sum over all JIT runs of one
`cargo test -p subscript-codegen -p subscript-cli` run, as thread
seconds, from a per-run log. The step wall time on this host varied by
up to 80 s between two identical runs (debug 264–345 s, release
369–474 s), so per-run sums are the cost evidence and wall times are not.

| Option | Hang in stress (6 threads, 1,000 runs) | Per run, release stress, no load | Suite sum, debug | Suite sum, release | Output on a crash |
|---|---|---:|---:|---:|---|
| current: `fork`, no `exec` | yes, at run 31 | 2.31 ms | 4.4 s / 1,129 runs | 5.7 s / 1,129 runs | kept, all runs |
| (a) `fork` + `exec` of a runner binary | no | 7.00 ms | — | — | kept, runs without native libraries |
| (b) `posix_spawn` of a runner binary | no | 6.17 ms | 61.2 s / 861 runs | 10.6 s / 861 runs | kept, runs without native libraries |
| (c) in process | no | 1.08 ms | 0.7 s / 1,125 runs | 0.3 s / 1,125 runs | lost |

Per-run figures include the compile in the parent. The runner prototype
compiles the program again in the child, so its sums include a second
compile: 52.8 s debug and 4.1 s release over 861 runs. Without that
compile, the `posix_spawn` cost per run is about 10 ms debug and 7.5 ms
release, against 3.9 ms and 5.0 ms for the current `fork`. A runner
binary launch alone costs 2.9 ms; a test binary launch with a filter
that matches no test costs 3.1 ms; `/usr/bin/true` costs 1.8 ms.

(a) and (b): the prototype sent runs with native libraries or a file
provider through the current `fork` (268 of 1,129 runs in the suite;
§44.10: a native symbol is an address in the caller's process, and a
fresh process cannot resolve it). With that split, both suites passed
with the same counts as the current form (debug 1,024 passed, release
1,023 passed). The hang stays possible for the 268 native-library runs.

(c): the `native_library` binary stopped on `SIGABRT`, because its
abort tests end the process that runs the program. The suite counted 7
fewer tests (1,017 debug, 1,016 release). `RunError::AbnormalTermination`
is unreachable in this form, as it is on Windows today (§44.10).

(d) Other forms, not prototyped:

- Worker threads through `pthread_create` instead of `std::thread`. This
  removes the observed row only. The panic, stderr, and environment rows
  stay.
- A single-threaded fork server, started once by `exec`, that compiles
  and forks for each run. Its `fork` is safe because it has one thread.
  It has the same native-symbol limit as (a) and (b).
- `--test-threads=1` for each binary that runs the JIT. Tests and the
  runtime can still start threads, and the suite loses its parallelism.

## Gate share

46 test binaries and 2 program binaries reach the forked runner. Of the
1,129 forked runs in one debug run, `golden` runs 394, `cemit` 192, and
`lifetime_operands` 64; each other test binary runs 44 or fewer. The CLI
binary `subscript`, under the CLI tests, runs 38, and `capture` runs 1. The forked runner's own cost
(the `fork`, the child run, and `waitpid`) sums to 4.4 s debug and 5.7 s
release across all test threads. The debug step is 460 s and the release
step is 500 s wall (gate records `20261010T175236Z`, `20261010T170400Z`).
The compile before each run sums to 110 s debug and 12 s release and
does not depend on the option, except the second compile in (a) and (b).

## Decisions a contract needs

1. The isolation form for the dev-tier run: an `exec` boundary, in
   process, or a single-threaded fork server.
2. How a native library crosses an `exec` boundary. Candidates: the
   runner builds the library's C sources into a shared library and
   resolves each symbol by name; or the test binary re-executes itself
   and rebuilds the library from a named registration. A symbol that
   exists only as a Rust function (the `native_library` abort tests, the
   archive fixture) needs one of these.
3. Whether §44.10 retention holds for every dev run or only for runs
   without native libraries, and whether a run with native libraries
   keeps the `fork`.
4. Which process checks the program. If the parent checks and the child
   compiles, no run compiles twice, and `RunError::Rejected` keeps its
   source.
5. How a caller finds the runner binary. `CARGO_BIN_EXE_*` exists only
   for integration tests of the same package; the CLI and other crates
   need an environment variable with a documented default.
6. Whether the CLI keeps the `fork`, because its parent is
   single-threaded at the `fork`.
7. Whether Windows gets retention from the same runner, for runs without
   native libraries.

## Implementation

Status: **implemented on 2026-10-11 against §190.** Measured on the host
of the measurement round.

### Red at the pin

The stress test is `a_worker_run_completes_while_other_threads_start_threads`
in `codegen/tests/dev_run_threads.rs`: 200 release runs of one Worker
program through `run_jit`, while 6 other threads start and join empty
threads without pause. At `2867fd14`, from a `git archive` copy with
this test added, 18 release trials ran with a time bound of 30 s or
60 s for each trial.

| Trials | Hung | Hung at run | Completed trials |
|---:|---:|---|---|
| 18 | 8 | 126, 46, 15, 194, 22, 71, 96, 33 | 10, each 200 runs in 1–2 s |

That is 8 hangs in about 2,603 runs, about 1 in 325. The rate of the
measurement round was about 1 in 32. With 200 runs, the pin hangs in 8
of 18 trials.

After the change, 10 release trials completed 200 runs each, in 2.04 s
to 2.14 s wall (about 10.2 ms for each run under the load).

The stress test in the tree is a regression guard, because the rule 1
check makes the hang unreachable. It runs 50 runs in release and 8 in
debug. It takes 0.53–0.89 s in release and 0.43–1.09 s in debug (3
runs each; the first run of a new binary is the slowest).

### Form

- Rule 1: `thread_count` (`codegen/src/jit/isolation.rs`) reads the
  Mach task thread list (`task_threads`) on Apple targets and counts
  `/proc/self/task` on Linux. On another Unix it gives no count, which
  is not 1. `run_jit_configured` selects the path from the
  count. `execute_entry_retained` reads the count again immediately
  before the `fork` and returns `RunError::Internal` that names rule 1
  if the count is not 1.
- Rule 2: the runner is a new process of the current executable,
  started with `posix_spawn`. The parent writes a request file
  (`codegen/src/jit/protocol.rs`): the source files (name, source,
  `dts`, `entry`), the enabled modules, the modules to poison, the
  allocation-failure number, and the freed-handle diagnostics flag. The
  arguments are the request path, the retained output path, and the
  outcome path. A file action opens the stderr file as descriptor 2.
  The environment is a copy of `std::env::vars_os` with the marker
  variable `SUBSCRIPT_DEV_JIT_RUNNER` added.
- The runner writes the outcome form of the forked child and ends with
  `_exit`. One function (`collect_child`) maps the wait status, stdout,
  and stderr for both forms, so the `RunError` kinds and the status
  texts are the same. The outcome form has one new tag:
  `RunError::UnresolvedForeignSymbol`, because a runner finds an
  unresolved foreign symbol in its own compile.
- Rule 3: after the check, a multithreaded run with a native library or
  a file provider returns `RunError::Internal` that names rule 3. An
  accounting run (`memory_accounting`) stays in process, as before.
- Rule 3, the re-read: for a run with a native library or a file
  provider, `thread_count_for_run` reads a count above 1 again every
  20 µs for up to 2 ms, before the rule 3 error and before the pre-fork
  rule 1 check. A run without such inputs reads the count once. The test
  `a_native_run_right_after_joins_runs_the_program` joins 4 threads and
  starts a native run at once. Without the re-read it failed in 2 of 3
  runs; with it, it passed in 5 of 5.
- Rule 4: the parent runs `check_program_with` and `runner_main`. The
  runner runs `compile_jit_with`, which checks again and compiles. No
  run compiles twice; a runner run checks twice.
- Rule 5: a `#[used]` static of `codegen/src/jit/isolation.rs` holds a
  constructor in `__DATA,__mod_init_func` (Apple) or `.init_array`
  (Linux with glibc; musl calls `.init_array` with no arguments). The
  loader calls it before `main` with `argc` and `argv`. If the marker
  variable is set and `argv[1]` is `--subscript-dev-jit-runner`, it
  removes the marker from the environment, runs the runner, and ends the
  process with `_exit`. The parent starts `/proc/self/exe` on Linux and
  `current_exe()` on Apple. The spawn path names the static, so every executable
  that can start a runner links it. The runner is always the same build
  as its parent: a libtest thread of `cargo test -p subscript-codegen
  --lib` and of `cargo test -p subscript-cli --lib` reaches it through
  its own test binary (acceptance 3). On another Unix, a multithreaded
  run returns `RunError::Internal`. A round 1 to 3 form used a separate
  `subscript-jit-runner` binary. `cargo test -p` of one package did not
  build it, so a run used a stale runner from an earlier build.
- Rule 6: `subscript run` calls `run_jit_configured` from its only
  thread, so it forks behind the rule 1 check. The CLI has no change.
- The thread count costs about 2.8 µs for each call (100,000 calls,
  debug and release, 2 threads). A forked run reads it twice.
- The parent borrows the caller's files for the request and writes the
  request once. The runner reads the outcome after `waitpid`; nothing
  polls.

### Test binaries that run programs with a native library or a file provider

Each binary below is `harness = false`. Its `main` runs the tests in the
two phases of rule 3 (`codegen/tests/support/main_thread.rs`). Phase 1
runs in parallel, one thread for each test with the test name, at most
`--test-threads` (or `RUST_TEST_THREADS`, or the available parallelism)
at a time, and the harness joins every thread. A phase 2 test starts
the test binary again (`--exact <name>`, with
`SUBSCRIPT_TEST_PHASE_2_CHILD` set). That child runs its one test on its
main thread; a child that selects any other number of tests fails. Both
phases share one set of workers and overlap, and a worker waits for
each child. Against the sequential phases, the overlap changed the
summed `finished in` of the moved binaries from 146.6 s to 136.7 s in
debug and from 70.7 s to 72.1 s in release. A non-Unix host runs every
test as phase 1. Filters apply first, so a filter
that matches no phase 2 test starts no child. The parent prints one
`test <name> ... ok` or `FAILED` line for each test, the output of a
failed child, and the `test result:` line with the totals. A failed
child fails the parent. Each root carries `#![deny(dead_code)]`, so a
test function that the phase list does not name fails the build. The
test `no_two_phase_binary_carries_a_libtest_attribute` reads every
`harness = false` target of codegen, cli, and examples, follows its
`mod` and `#[path]` files, and fails on a `#[test]` or `#[ignore]`
attribute, which the harness never runs. Its control scans a source
that holds both. A binary that excludes
itself on windows-msvc with an inner `cfg` gets its `main` from
`tests/main_thread/<name>.rs`.

Phase 2 is the measured smallest set. With every test in phase 1, the
tests below failed, each with the rule 3 error. One exception:
`jit_output_file_override_still_retains_child_process_output` failed
because its child process runs `jit_output_file_override_child` in
phase 1. The child target is in phase 2, and the parent test stays in
phase 1. `dev_run_threads` holds a phase 1 native
run that gets the rule 3 error, with the phase 2 control.

The times below come from the cost runs, with the example hosts built
before the timed run.

| Package | Binary | Phase 2 tests | Debug `finished in`, pin → now | Release, pin → now |
|---|---|---|---:|---:|
| codegen | `abi_pressure` | `header_only_boundary_value_runs_in_both_tiers`, `boundary_gate_in_both_tiers` | 3.4 → 3.5 s | 1.3 → 1.3 s |
| codegen | `boundary_module_invariance` | its 1 test | 4.2 → 4.3 s | 3.2 → 3.2 s |
| codegen | `boundary_scratch_breadth` | its 1 test | 37.0 → 36.8 s | 2.4 → 2.3 s |
| codegen | `cemit` | `trap_corpus_entries_match_dev_stdout_on_both_tiers` | 17.9 → 20.4 s | 15.7 → 16.0 s |
| codegen | `dev_run_threads` | the two native controls | new, 0.4 s | new, 0.5 s |
| codegen | `file_module` | `a_pre_init_hook_provider_is_visible_to_the_module_initializer` | 1.0 → 1.0 s | 0.8 → 0.9 s |
| codegen | `function_value_reference` | `boundary_function_field_reads_the_same_pair_as_a_local_copy`, `nullable_boundary_map_get_or_accepts_a_null_default` | 0.9 → 0.9 s | 0.8 → 0.7 s |
| codegen | `golden` | 24 of 35, with `jit_ship_c_aot_and_golden_agree_byte_for_byte` and `narrow_corpus_entries_match_across_tiers_before_golden_comparison` | 35.2 → 42.0 s | 25.9 → 27.0 s |
| codegen | `interop` | all 21 | 3.0 → 3.2 s | 2.6 → 2.7 s |
| codegen | `module_init_routes` | `foreign_callback_rejects_early_read_and_runs_after_global` | 1.1 → 1.6 s | 1.0 → 1.3 s |
| codegen | `module_init_values` | `a_host_call_can_initialize_a_global`, `a_direct_body_can_call_a_host_to_initialize_a_global` | 0.8 → 0.9 s | 0.7 → 0.7 s |
| codegen | `native_library` | `non_unwinding_panic_surfaces_output_already_produced`, `no_opt_in_hard_signal_returns_retained_output_on_both_tiers`, `static_archive_link_input_follows_translation_units_on_all_tiers`, `jit_output_file_override_child` | 0.5 → 0.5 s | 0.4 → 0.4 s |
| codegen | `shared_narrowing` | `narrowed_boundary_field_and_global_stores_keep_the_box`, `as_cast_null_traps_keep_their_runtime_identity` | 1.6 → 1.7 s | 1.4 → 1.4 s |
| cli | `read_root` | 21 of 23 | 4.9 → 3.9 s | 4.0 → 3.0 s |
| examples | `gate` | `every_example_matches_dev_jit_ship_c_aot_and_golden`, `every_phase_gate_program_matches_dev_jit_ship_c_aot_and_golden` | 6.1 → 6.2 s | 2.7 → 2.5 s |

The pools of the `golden` and `cemit` sweeps keep the ship-tier runs and
the dev runs without a library. A dev run with a library runs on the
main thread of the phase 2 child after the pool joins its threads.

### Counts

| Run | Pin `2867fd14` | Now |
|---|---:|---:|
| codegen, debug | 915 + 1 ignored | 928 + 1 ignored |
| codegen, release | 914 + 1 ignored | 927 + 1 ignored |
| cli without `gate.rs`, debug and release | 94 | 95 |
| examples, debug and release | 7 | 7 |

The 13 new codegen tests are the 7 tests of `dev_run_threads` and 6
unit tests of `isolation.rs` and `protocol.rs`. The new CLI test is the
in-process run of acceptance 3. The pin counts come from
the `git archive` copy, where 7 tests fail because the copy has no
`.git` (`docs`, `host_entry`) or no `node_modules` (`lir_completion`,
the CLI `commands` test).

### Cost

The per-run log method of the measurement round: the sum of the time of
each `run_jit_configured` call (check, compile, and run, both forms),
over one codegen, CLI (without `gate.rs`), and examples test run, with
the 50-run stress test. A phase 2 child inherits the log variable. The
runs were adjacent, pin first. The instrumentation was reverted.

| Run | Dev runs | Per-run sum | Wall, codegen + CLI |
|---|---:|---:|---:|
| pin, debug | 1,131 | 121.1 s | 293 s |
| now, debug | 1,180 | 115.6 s | 307 s |
| pin, release | 1,131 | 15.1 s | 137 s |
| now, release | 1,222 | 15.8 s | 140 s |

The form that ran phase 2 on the main thread of the parent, one test at
a time, cost about 73 s debug and 57 s release of codegen + CLI wall.

### Rare shapes, recorded

- On a Unix other than Apple and Linux with glibc, no runner
  constructor exists, and a multithreaded dev run returns
  `RunError::Internal`. On a Unix other than Apple and Linux, the thread
  count is also unknown, so every dev run there returns it.
- The panic message of a runner names the thread `main`; a forked child
  named the thread of the caller. No test reads the thread name.

### Open

- MINOR 10: `isolation.rs` and `entry.rs` hold `expect` calls on
  invariants of a live temporary file. They do not fail for a valid
  run.
- MINOR 11: if `waitpid` returns an error other than `EINTR`,
  `collect_child` returns `RunError::Internal` and does not reap the
  child.
- D: the request form encodes `SourceFile` and `CheckOptions` field by
  field. Both are `non_exhaustive` structs of another crate, so a new
  field does not fail the build here; the round-trip test compares the
  `Debug` forms of the fields it sets.
- E: on macOS, the runner starts `current_exe()` by path. If the
  executable file is replaced during a run, the runner is the new build.
- F: `#![deny(dead_code)]` does not see a `pub fn` at a crate root, so
  an unlisted test function that is `pub` is not reported.
- G: `--nocapture` has no effect on phase 2: the parent prints a child's
  output only for a failure. A process that a phase 2 test starts
  inherits `SUBSCRIPT_TEST_PHASE_2_CHILD`.
