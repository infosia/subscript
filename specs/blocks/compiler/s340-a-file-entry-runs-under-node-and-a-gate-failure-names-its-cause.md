<!-- §340 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 340. A file entry runs under `node`, and a gate failure names its cause

*(Added 2026-11-19.)* Origin: rank 21 of `specs/tracking/backlog.md`
(§185.4 item 2, `specs/tracking/jit-runner-fork.md` Open MINOR 10 and
11, §325.5 item 5, and one release gate failure of
`cli/tests/read_root.rs`). On 2026-11-19 the owner selected rank 21.

Problem, measured at `eaaecee4`:

1. **No `node` run of the file module.** `a355` is `js-comparable: no`
   (C25). The `node` runner (`corpus/node/run-js-corpus.cjs`) runs
   every entry in the repository root, and gives an entry no scratch
   directory. So no gate compares the read and write path of
   `node:fs/promises` with `node`.
2. **`bound-call` gives no valid comparison.** One release run on
   arm64 macOS: script 8,834 ns per region, mimic 6,375 ns.
   "decomposition: invalid; script and mimic differ by 27.84% (> 20%)",
   exit 2. The mimic is the C copy of the emitted script region, so a
   difference above the limit means that the copy and the emitted C
   differ. The same run gave "no-trap measured-but-invalid; spread
   79.25%".
3. **A run error holds an `expect`.** `codegen/src/jit/entry.rs`
   (the child stderr and the protocol file) and
   `codegen/src/jit/isolation.rs` (the runner request file) call
   `expect` on a live temporary file (MINOR 10). Core principle 5:
   library code does not panic.
4. **A failed wait leaves a child.** If `waitpid` returns an error
   other than `EINTR`, `collect_child` returns `RunError::Internal`
   and does not reap the child (MINOR 11).
5. **A thread-count failure does not name the thread.** The release
   gate of 2026-11-18 (`4239b6af`) failed
   `a_pointer_target_is_written_back` once with the §190.1 rule 3
   error "(the process has 2 threads)". The next run passed. The
   message does not say which thread. On arm64 macOS, a C program that
   starts and joins one thread, then counts with `task_threads`, saw
   the joined thread in the count in 404–3,767 of 5,000 cycles, for
   at most 0.028 ms, with no load and with 16 busy processes on 8
   cores. The 2 ms re-read covers that. So the second thread was a
   live thread, not a joined one, and its source is not known.

### 340.1 Rules

1. **A file entry runs in its own directory under `node`.** The `node`
   runner runs an entry whose header has `file-provider: scratch` with
   a new empty directory as its current directory, and removes the
   directory after the entry. A new accept entry writes and reads text
   and the bytes of a text file through `node:fs/promises`, and is
   `js-comparable: yes`. Its output equals `node`. It uses no form of
   C25 items 1–3 in its output: it prints a byte count and byte
   values, not a byte array, and it has no error path.
2. **The mimic is the emitted region.** The `bound-call` mimic subject
   holds the C that the emitter gives for the script region, with the
   same operations in the same order. If the emitted region and the
   mimic differ, the mimic changes. If the emitted region holds an
   operation that no contract section asks for, stop and report it.
3. **A run error is an error.** Each `expect` of item 3 becomes a
   `RunError::Internal` that names the file. No `expect` or `unwrap`
   stays on a run path of `codegen/src/jit/` outside tests.
4. **A failed wait reaps the child.** If `waitpid` fails with an error
   other than `EINTR`, `collect_child` stops the child with `SIGKILL`,
   waits for it, and then returns the error.
5. **The rule 3 error names each other thread.** The §190.1 rule 1 and
   rule 3 messages list each thread other than the caller: its name
   (the `pthread` name on Apple targets, `comm` on Linux), or
   `unnamed`, and on Apple targets the label of its dispatch queue if
   it has one. The count and the decision do not change.

### 340.2 Acceptance

1. Rule 1: the new entry passes `tsc`, the development tier, the ship
   tier, and the `node` gate. A test of the runner shows that two
   file entries do not see each other's files.
2. Rule 2: three release runs of `bound-call` on arm64 macOS, with no
   other build: each prints "decomposition: valid". The report names
   each difference between the emitted region and the mimic at the
   pin.
3. Rule 4: a unit test gives `collect_child` a wait error other than
   `EINTR` for a live child, and shows that the child is reaped. The
   test reaches the error through the wait function that
   `collect_child` calls, not through the record that it returns.
4. Rule 5: a test starts a named second thread, and the rule 3 message
   names it. A control without the second thread runs.
5. The implementation commit records the macOS measurement of
   problem 5 in `specs/tracking/jit-runner-fork.md`.

### 340.3 Sections this one amends

- §185.4 item 2 closes. §325.5 item 5 closes.
- `specs/tracking/jit-runner-fork.md` MINOR 10 and MINOR 11 close.

### 340.4 Phase Review

Form.

- Rule 1: `corpus/node/run-js-corpus.cjs` resolves the entry path,
  makes a new directory for an entry with `file-provider: scratch`,
  and in `finally` restores the directory and removes the new one.
- Rule 2: the `bound-call` mimic, no-trap, and hoisted subjects hold
  the operations of the emitted region (`benchmarks/src/bin/bound-call.rs`).
- Rules 3 and 4: `codegen/src/jit/output.rs`, `entry.rs`, and
  `isolation.rs` return errors, and the parent checks each file
  before the `fork`. `collect_child` stops and reaps the child after a
  wait error.
- Rule 5: `codegen/src/jit/thread_names.rs` collects the names only
  when the count is not 1, so a run that passes costs the same.

Measured at the pin `1664091b` and after:

| Item | Pin | After |
|---|---|---|
| `a492` under `node` | output equal, `text.txt` and `copy.txt` left in the repository root | output equal, no file left |
| `bound-call`, release, arm64 macOS | script 8,834 ns, mimic 6,417 ns, 27.36%, exit 2 | 2.29%, 0.94%, 0.01%, each "decomposition: valid", exit 0 |
| rule 3 message with a named second thread | "(the process has 2 threads)" | names `s340-native` |

At the pin the mimic did not hold these operations of the emitted
region: the boundary scope mark, the borrow, and the scope release
(§323 rules 1 and 2), the trap checks after `bnMoreSamples`,
`bnReport`, and `bnBindGroupRelease`, the wrapping index update, and
the root clears. Its trapped path also released the group, and the
emitted region does not. One later attempt failed on the spread of
one hoisted sample (78.73%); its decomposition was valid (0.48%).

Each of the tests of rules 1, 4, and 5 fails with its subject changed
(the pin runner; the reap removed; the names removed). The Linux form
of rule 5 is not compiled here: the target is not installed. The owner
runs the Linux gate.

*(Amended 2026-11-22: the x86-64 Linux full gate at `573104df` exits
0. The Linux form of rule 5 compiles, and its test names `s340-native`
in the debug and the release step. Evidence:
`specs/tracking/linux-portability.md`.)*

One fresh review found no CRITICAL or MAJOR finding. Its five MINOR
findings: the table header of the tracking note is fixed; the other
four are in §340.5.

### 340.5 Open

1. Contrived. The queue label read copies one byte for each call and
   has no length limit. A queue that is released during the read can
   make the failure path slow. It cannot stop the process: the kernel
   does the copy.
2. Contrived. If the host sets `SIGCHLD` to `SIG_IGN`, `waitpid`
   returns `ECHILD`, and `collect_child` still sends `SIGKILL` to a pid
   that is free.
3. Contrived. `TemporaryFile::path` gives an error that does not name
   the file. No caller reaches it, because `take_parts` consumes the
   value.
4. Style. `codegen/src/jit/isolation.rs` declares `thread_names` with
   `#[path]` to a sibling file.
