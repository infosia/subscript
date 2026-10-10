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
