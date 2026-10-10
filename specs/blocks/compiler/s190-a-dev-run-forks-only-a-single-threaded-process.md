<!-- §190 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 190. A dev run forks only a single-threaded process

*(Added 2026-10-11.)* Origin: a full-gate release step stopped at its
3,600 s bound. On 2026-10-11 the owner selected this section and the
form of rules 1–3. The measurement round at `2867fd14` is
`specs/tracking/jit-runner-fork.md`.

Problem: `execute_entry_retained` (`codegen/src/jit/entry.rs`) calls
`fork` and runs the program in the child without `exec`. The child runs
Rust code that takes std locks: a thread start, a panic message, stderr
output, and an environment read. If another parent thread holds one of
these locks at the `fork`, the child waits forever. A test binary
starts one thread for each test, so its process is multithreaded at
each `fork`. A stress test hung in about 1 run of 32 in the
measurement round, and in about 1 run of 325 in the Red trials of
round 1. One gate step in 28 hung.

### 190.1 Rules

1. **A `fork` needs one thread.** The dev tier calls `fork` only when
   the calling process has exactly one thread. A check before each
   `fork` reads the thread count from the platform. If the count is
   not 1, the run does not fork. This makes the class unreachable: a
   single-threaded parent holds no lock that another thread took.
2. **A multithreaded caller uses a runner.** If the caller has more
   than one thread and the run has no native library and no file
   provider, the dev tier starts a runner process with `posix_spawn`
   (6.17 ms per run, against 7.00 ms for `fork` and `exec`). The
   runner compiles and runs the program, and
   the parent reads its output after any termination. §44.10 retention
   holds for these runs.
3. **A native library needs a single-threaded caller.** A native symbol
   is an address in the caller's process (§44.10), so a fresh process
   cannot resolve it. If a run has a native library or a file provider
   and the caller has more than one thread, the run returns
   `RunError::Internal` with a message that names this rule. It does
   not fork, and it does not run in process. The platform can count a
   joined thread for some microseconds after the join (up to 36 µs
   measured). So before this error, and before the rule 1 check of
   such a run refuses the `fork`, the dev tier reads the count again
   for up to 2 ms. A run without a native library or a file provider
   does not wait. A test binary that runs
   such programs lists each test in one of two phases. A phase 1 test
   starts no such run and runs on a thread of the harness. A phase 2
   test runs in its own process: the harness starts the test binary
   again for that one test, and that process runs the test on its main
   thread. Both phases share the test thread limit, and they can run
   at the same time. If a test is in the wrong phase, its run returns
   the rule 3 error, so the list cannot be wrong silently. A test
   function that the list does not name fails the build. That property
   comes from the binary itself, so a plain `cargo test` holds it, and
   no gate argument is necessary. On a non-Unix host, the harness runs
   every test as phase 1.
4. **One compile.** The parent checks the program and does not compile
   it, so `RunError::Rejected` keeps its source. The runner compiles
   the program. No run compiles twice.
5. **The runner is the caller's executable.** The dev tier starts the
   current executable again, with a marker variable. A constructor in
   the codegen library reads the marker before `main` and runs the
   runner. So the runner is always the same build as the parent, and a
   host or a test needs no second binary. If the executable cannot
   start, the run returns `RunError::Internal`.
6. **The CLI.** `subscript run` starts no thread before the run, so
   rule 1 lets it fork, with native libraries included. The rule 1
   check guards it.
7. **An accounting run stays in process.** A run with
   `memory_accounting` runs in the caller's process, as before. Rules
   2 and 3 do not apply to it.
8. **Not in this section.** Windows keeps the in-process run (§44.10).
   No fork server. No change to the ship tier or to the interpreter.
   Worker threads keep `std::thread`.

### 190.2 Acceptance

1. Red at the pin (core principle 10): a release stress test runs one
   Worker program through `run_jit` while six other threads start and
   join threads without pause. At `2867fd14` it hangs within a stated
   number of runs. After the change it completes its runs within a
   stated bound. Its gate cost is measured and stated (core principle
   15).
2. A test builds the violating form of rule 3: it keeps a second
   thread alive and starts a run with a native library. The run
   returns the rule 3 error and does not fork. A same-shape control
   with one thread runs the program.
3. A test of rule 5: a run from a libtest thread of a test binary
   reaches the runner through the test binary itself. A test of rule 3:
   a test function that the phase list does not name fails the build,
   and a check fails on a `#[test]` attribute in a two-phase binary.
4. Every test that ran before passes, with the same counts. The native
   library abort tests keep `RunError::AbnormalTermination`.
5. Cost: the sum of the dev-run time over one codegen and CLI test run,
   debug and release, against the pin (the note's per-run log method).
6. The tracking note records each test binary that runs native-library
   or file-provider programs, the tests of its second phase, and the
   wall time of each such binary against the pin.
