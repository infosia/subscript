<!-- §342 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 342. A Context collects automatically

*(Added 2026-11-21.)* Origin: on 2026-11-21 the owner started a trial
of automatic collection on the branch `gc-auto`. The measurement round
G0 is in `specs/tracking/automatic-collection.md`. This section changes
design invariant 2. `Context.free` stays (owner decision,
2026-11-21): it is the manual level of rule 4a.

Problem, measured at `ca3fc9ed`:

1. **A program must release memory by hand, or the host must collect.**
   The `tree` workload carries a recursive `free` function for that
   purpose only. Without it, and without a host collection, live bytes
   grow with each round.
2. **The existing collector already serves a collection at any script
   position.** A prototype put a poll at each loop backedge and each
   function entry, and made each poll collect. In the three tiers, no
   live reference was collected: 498 accept and trap entries, and 78
   native, hook, and file entries on the two native tiers.
3. **A poll in code that cannot allocate has a large cost and serves
   nothing.** With a poll at each backedge and entry, and no
   collection, the ship tier read 1.94 on `callbacks` and the dev JIT
   read 2.51 on `mandelbrot`. With polls only where an allocation can
   occur, each workload read 0.98 to 1.01.
4. **The poll where a collection runs is not equal in the three
   tiers.** The ship tier counts the size-class capacity as live bytes,
   and the dev tier counts the payload.
5. **A collection at each host step is too expensive.** A program that
   holds one failed handle for 2,000 steps with 200,000 live objects
   took 0.02 s in the dev JIT. With one collection at each outermost
   exit while a failed unobserved task exists, it took 20.5 s.
6. **Four runtime operations hold a reference in a Rust local while
   script code runs.** With a function-value callback that empties the
   receiver and collects, `filter`, `find`, `findLast`, `Map.groupBy`,
   and `sort` give a wrong value in the two native tiers. The
   interpreter gives the correct value. An explicit `Context.collect()`
   in the callback reaches this at `ca3fc9ed`. With automatic
   collection, the callback needs only the change of the receiver.
   This section records the shape and does not change it (§342.5
   item 4).

### 342.1 Rules

**Lifetime**

1. **A reachable object stays valid.** An object that no root reaches
   is reclaimed at a time that this contract does not state. A program
   that never collects is correct, merely larger.
2. **An automatic collection is not observable from script code.** The
   output, the traps, and the trap positions of a program do not depend
   on the polls where collections run. Rules 12 to 16 give the one
   exception a time that does not depend on the polls.
3. **No collection moves an object.** Invariant 1 does not change.
4. **Cleanup of an external resource does not use reachability.** It
   uses `using` (§60) and the release of a registration (§111).

4a. **The host selects one of three levels.** Automatic is mode 1
    (rule 17). Host-timed is mode 0 with `subscript_rt_ctx_collect` at
    a point that the host selects. Manual is mode 0 with
    `Context.free` in script code, and no collection. `Context.free`
    and `Context.collect()` are valid at each level. A collection of a
    Context runs only inside a call into that Context, on the thread
    of that call: no collector thread exists. For the Context of the
    host, that call is the host's. For a worker Context, it is the
    runtime's call on the worker thread (§39). The documents state the
    host's control of the pause first, then the three levels, then the
    facts that hold at each level (rules 2 and 3, and this rule).

**Trigger**

5. **An allocation never collects.** A successful allocation sets the
   pending flag of the Context if live bytes exceed the threshold.
6. **The threshold follows the live set.** After each collection the
   threshold is live bytes plus the larger of live bytes and the
   minimum headroom. The default minimum headroom is 16 MiB.
7. **A poll serves the pending flag.** A poll reads the flag. If the
   flag is set, the poll runs one automatic collection through the
   collection path of its tier, and the collection clears the flag.
8. **The outermost exit serves the pending flag.** When script depth
   returns to zero and the Context has no trap, a set flag runs one
   automatic collection, after the steps of rules 14 and 15.

8a. **The host marks the outermost exit.** An exported entry does not
    mark it. A host that calls an export brackets the call with
    `subscript_rt_ctx_enter_script` and `subscript_rt_ctx_exit_script`
    (§18, §115.4). Rules 8, 14, and 15 run at that exit only. The
    header and the C tutorial state it. Measured in the ship tier with
    mode 1: 10,000 calls of an entry that allocates in straight-line
    code left 83,530,016 live bytes with no bracket, and 16 with it.

**Polls**

9. **The poll is one LIR instruction kind.** A LIR pass places it. The
   three tiers lower the same polls. The native tiers lower a poll to
   one load of the flag and one branch; the call is on the cold path.
   A poll carries no trap site on the path where the flag is clear.
10. **The LIR carries the allocation fact.** Each intrinsic operation
    and each built-in method of the operation table states if it can
    allocate in the Context. Each instruction kind has the fact by a
    total match. A function can allocate if it has an instruction that
    can allocate or a call that can allocate. A call to a script
    function has the fact of its callee. An indirect call and a foreign
    call can allocate.
10b. **A push into reserved capacity is an operation of its own.** A
    static-callback array operation (`map`, `filter`, and the others
    that lower to a script-level loop) allocates its result with a
    capacity before the loop. Its push is the built-in method
    `ArrayPushReserved`, which states "cannot allocate". The lowering
    emits it only where the trip count of the loop cannot exceed the
    reserved capacity. `ArrayPush` states "can allocate". Measured with
    one method for both: the backedge poll of the `map` and `filter`
    loops read 2.16 on `callbacks` in the ship tier with the mode off.
10a. **The interpreter verifies the fact.** While the interpreter
    executes an instruction or an operation that states "cannot
    allocate", an allocation in the Context is a runtime error of kind
    `Internal`. The stated fact and the runtime are two separate
    derivations (core principle 9).
11. **Placement.**
    a. A backedge has a poll if a block of its natural loop can
       allocate. The poll is before the terminator of the source block.
    b. A function has an entry poll if it can allocate, and it is in a
       call-graph cycle, or has its address taken, or is async, or is a
       generator. The entry poll is after the parameter stores.
    c. A resume of an async function or of a generator reaches a poll
       before it can allocate an unbounded count of objects.
    d. Each allocation path reaches a poll, or the outermost exit,
       after a bounded count of allocations. The bound of straight-line
       code is its size.

**Failure of an unobserved task**

12. **An automatic collection does not trap.** If it releases the last
    owner of a failed unobserved task (§70, §116), it records the
    failure in the Context. The recorded error object stays reachable.
13. **The existing reports do not change.** `await`, the last-owner
    release at a known position, the explicit `Context.collect()`, and
    `subscript_rt_ctx_collect` report as before. An explicit collection
    reports one failure: the lowest task id (§177) of the recorded
    failures and of the failures that it finds itself, at the position
    of a failure that it finds itself. The other records are dropped,
    as a trap that is set drops a later trap.
14. **A detection collection runs at the outermost exit, on a schedule
    of exit counts.** The Context holds an interval, initially 1, and a
    countdown, initially 1. At each outermost exit with no trap:
    a. If no recorded failure and no failed unobserved task exists, the
       interval and the countdown become 1. A task whose failure the
       Context reported is not a failed unobserved task.
    b. Otherwise the countdown decreases by 1. At zero, one detection
       collection runs.
14b. **The test of rule 14a costs nothing while no task failed.** The
    Context holds a flag that a failed unobserved task can exist. Each
    site that stores a failure in a task sets it. An outermost exit
    reads the tasks only if the flag is set, and clears the flag when
    it finds no failed unobserved task. A debug build compares the
    clear flag with the tasks at each outermost exit. Measured with
    the tasks read at each exit, in a release build: 128 ns for each
    exit with 100 suspended tasks, and 13,790 ns with 10,000, against
    11 ns and 6 ns in mode 0.
15. **A detection collection reports, or backs off.** It is a full
    collection. If a recorded failure exists after it, the Context
    reports the one with the lowest task id as an `UncaughtException`
    trap at the throw position. If none exists, the interval becomes
    the smaller of twice its value and 1,024, and the countdown becomes
    the interval.
16. **The exit count is equal in the three tiers.** The interpreter
    marks the outermost exit at the same points as the native runner,
    through one shared operation of the Context. A test compares the
    count.

Cost of rules 14 and 15: one collection for each period in which a
failed task is unobserved at an outermost exit, then one at exits 3, 7,
15, and so on, and one each 1,024 exits after that.

**Host control**

17. **The host sets the mode.** `subscript_rt_ctx_set_collection(ctx,
    mode, min_headroom_bytes)` sets the mode and the minimum headroom.
    Mode 0 is off: no allocation sets the flag, and rules 14 and 15 do
    not run. Mode 1 is automatic, and is the default. Mode 2 makes each
    poll and each outermost exit collect, for tests. A
    `min_headroom_bytes` of 0 keeps the default. The call returns 0, or
    -1 for an unknown mode or a script depth that is not zero. The call
    runs no collection, so the threshold keeps its base: it is the
    live bytes after the last collection plus the larger of those
    bytes and the new minimum headroom. In mode 1 the call sets the
    pending flag if live bytes exceed that threshold. A host that
    switches the mode at each frame gets the same collections as a
    host that leaves mode 1 set.
18. **A worker Context takes the mode and the headroom of its parent
    at the spawn.**
19. **A host that keeps a pointer after the borrow ends has a defect.**
    The borrow rules do not change (§28 rule 2, §323 rule 1,
    `collisions.md` Q4 and Q15). The C tutorial states them for an
    array, a string view, a struct passed as script memory, and a
    pointer passed to an async completion function (closes §323.4
    item 4). The header states the lifetime of the result of
    `subscript_rt_worker_poll`, `subscript_rt_worker_inbox_wait`, and
    `subscript_rt_worker_inbox_poll`: it is valid until the next call
    into the Context that can run script code or collect.
20. **The memory metrics are equal in the tiers only after an explicit
    collection.** Between explicit collections,
    `subscript_rt_ctx_live_allocations` and
    `subscript_rt_ctx_live_bytes` depend on the polls (§18.2d).

### 342.2 Acceptance

Each measurement names the result that fails the section.

1. **Stress.** A test runs each accept and trap entry in the three
   tiers with mode 2 and compares against the golden. An entry that
   the test does not run has a stated reason in the test
   (`a22-matrix-propagation`: cost). The test states its cost; it runs
   in `tools/gate.sh full` only. Fail: a difference.
2. **Reclamation in one host call.** The `tree` workload without its
   `free` function prints 3932130 in the three tiers, and live bytes
   after the last collection are under the bytes of three trees. Fail:
   a wrong checksum, or live bytes that grow with the round count.
3. **Reclamation across host calls.** A host calls 10,000 times an
   entry that allocates in straight-line code. Live bytes stay under
   the threshold of rule 6 plus one call. Red at the pin: live bytes
   grow with the call count.
4. **Cost.** `perf-gate --gate` passes with its present limits. With
   mode 1 and no collection, no workload moves by more than the noise
   of §213.1 against the pin. Fail: a workload over its limit.
5. **Detection schedule.** The held-handle program of problem 5 runs
   at most 11 detection collections in 2,000 exits, in each tier, and
   prints `caught`. Fail: a count over 11, or a trap.
6. **Failure delivery.** New trap entries:
   - `t116`: a failed task whose holder is unreachable, with no
     explicit collection. The trap is `UncaughtException` at the throw
     position at the first outermost exit. Red at the pin: no trap.
   - `t117`: two such tasks; the lowest task id is reported.
   - `t97`, `t98`, and `t107` change: the trap position is the throw
     position, because an `await` ends a step before the explicit
     collection. Their headers and pins state it.
7. **Allocation fact.** The interpreter runs the accept and trap
   corpus with the check of rule 10a. Fail: an `Internal` error.
8. **Exit count.** The test of rule 16 compares the interpreter and
   the dev JIT, for each accept and trap entry that both run to the
   end with no trap. A run that ends in a trap gives no count, and
   the ship tier has no function that reads the count. For those, the
   evidence is the golden of acceptance 1 and 6 in the three tiers:
   the exit decides where `t97`, `t98`, `t107`, `t116`, and `t117`
   trap.
9. `tools/gate.sh full` passes.

### 342.3 Sections this one amends

- `CLAUDE.md` design invariant 2, and plan §2: the text of rule 1 to
  rule 4a replaces "no implicit GC".
- `collisions.md` Q7: `Context.collect()` is the explicit collection;
  the sentence "never runs unbidden" goes.
- §7, §18.2d, §22, §70.3, §170, §172: each statement that only an
  explicit collection releases an unreachable object now reads "a
  collection".
- §177 and §116: the report position of a failure that a detection
  collection finds is the throw position.
- §323.4 item 4 closes (rule 19).
- `stdlib.md` §15.5a and `warnings.md` W001 do not change in this
  section. W001 states that the Context grows until a collection,
  which is false in mode 1; its change is a section of its own.

### 342.4 Rounds

| Round | Content |
|---|---|
| 1 | Runtime: rules 5 to 8 and 12 to 18, the host API, unit tests. |
| 2 | LIR: rules 9 to 11, the three lowerings, the interpreter check. |
| 3 | Rule 16, the corpus entries, the stress test, acceptance 2, 3, and 5. |
| 4 | Documentation, the example host, a reload session that sets the mode, acceptance 4 and 9. |

### 342.4a Phase Review

Form.

- `runtime/src/context/collection.rs` holds the mode, the threshold
  and its base, the pending flag (one byte in the fixed prefix of the
  Context), the recorded failures, the schedule of rules 14 and 15,
  and the flag of rule 14b. `exit_script` is `exit_script_with_tasks`
  and `finish_exit_service`; the interpreter calls the same pair with
  its own root walk (`codegen/src/interpreter/exits.rs`).
- The allocation fact is a field of each row of the operation table
  and a total match on each HIR intrinsic enum, on `BuiltinMethod`, on
  `InstructionKind`, and on `Terminator`. The fact of a function is an
  analysis result (`codegen/src/lir/allocation.rs`), not a stored
  field, so the LIR text does not carry it.
- `InstructionKind::CollectionPoll` has no operand, no result, and no
  trap site. The pass is `codegen/src/lir/collection_poll.rs`. The
  verifier (`verify_collection_poll.rs`) uses strongly connected
  components and does not call the pass.
- The emitted C tests the byte through the macro `SUBSCRIPT_UNLIKELY`
  of the prelude (`__builtin_expect` under clang and gcc, the plain
  condition under MSVC). The MSVC form is not built on this host.
- A dev run and a reload session take the mode in
  `RunConfig::collection`. A session keeps it for each Context that it
  builds.

Measured.

| Item | Result |
|---|---|
| Stress sweep, mode 2, 594 entries | dev JIT 591 of 591 equal, ship tier 589 of 589, interpreter 486 of 486; 29,889 and 12,295 automatic collections in the dev JIT and the interpreter |
| Entries that the sweep does not run | dev JIT 3, ship tier 5, interpreter 108; each has its reason in `exclusion()` of `codegen/tests/collection_stress.rs` |
| Cost of the sweep | 68 s in a debug build, 36 to 49 s in a release build; it runs in `tools/gate.sh full` only |
| Exit count, interpreter against dev JIT | 398 of 398 entries equal |
| Rule 10a over the corpus | no `Internal` error; the interpreter sweep moved from 8.33–8.40 s to 8.57–8.62 s |
| Tree without `free`, ship tier, polls only (no bracket) | mode 0: at least 29 trees live; mode 1 with 64 KiB headroom: under 3 |
| 10,000 host calls of a straight-line entry, ship tier | mode 0: 83,530,016 live bytes; mode 1: 16 |
| Held failed handle, 2,000 steps | 2,003 exits and 10 detection collections in the interpreter and the dev JIT; `caught` in the three tiers |
| Poll cost, mode off, release, against the commit before the polls | `callbacks` ship 37.0 against 37.0–37.1 ms; `particles`, `sort`, `fib-recursive`, `mandelbrot`, `fib-loop` 0.99 to 1.00 in both native tiers |
| Outermost exit with no failed task, mode 1 (rule 14b) | 5.2 ns with 100 and with 10,000 suspended tasks |
| `perf-gate --gate`, one quiet run | each criterion met: `a22` ship 1.44x, `a22` dev 20.01x, `collect` ship 1.18x, `collect` dev 2.77x |
| LIR golden | 869 `CollectionPoll()` lines added, then 7 removed and 8 pushes renamed by rule 10b |
| Corpus | `t97`, `t98`, `t107` trap at `11:55`; `t116` and `t117` are new and Red at `ca3fc9ed` (no trap) in the three tiers; no other golden moved |

The `perf-gate` figure of `a22` on the ship tier at the pin was not
measured in this section.

Phase Review (a subagent with no context, on the cumulative diff): no
CRITICAL, 4 MAJOR, 6 MINOR. Each MAJOR is closed.

| Finding | Close |
|---|---|
| MAJOR, realistic: `subscript_rt_ctx_set_collection` computed the threshold from the present live bytes, so a host that switched the mode at each frame never collected | rule 17 states the base; a 2,000-frame test gives the same 400 collections as mode 1 left set (0 before) |
| MAJOR, realistic: a reload after an initializer trap lost the mode | the session keeps the mode; a test shows it by behaviour |
| MAJOR: no test showed that a ship-tier poll collects | the test without the bracket, and a ship-tier mode control in the sweep |
| MAJOR: acceptance 8 was wider than its test | acceptance 8 states the scope that the test has |
| MINOR: the tiers differed on a reported aggregate | rule 14a; 0 detection collections after the report (4 before) |
| MINOR: three public functions had no direct test | three runtime tests |
| MINOR: the cost of the task scan at each exit | rule 14b |
| MINOR: text in the header, the two tutorials, a test comment, and `stdlib.md` | corrected |

`tools/hygiene.sh`: exit 0.

`tools/gate.sh full` on the tree before this record: exit 0, debug
3905 passed, release 3886 passed, 0 failed, 5 ignored in each. No
golden moved.

### 342.5 Open

1. The conservative scan can keep an unreachable object alive if an
   integer equals its address (`runtime/src/context/memory.rs`, the
   payload scan). Exact reference layouts are not in this section.
2. A reference-count cycle of async frames is not reclaimed (§70.3.6).
3. One collection of about 4 MiB of live bytes took about 6 ms in the
   `callbacks` workload on the ship tier. The cause is not traced.
4. Contrived, MINOR (the review rule of invariant 6): a function-value
   callback of `filter`, `find`, `findLast`, `Map.groupBy`, or `sort`
   that removes or replaces elements of the receiver. The runtime
   holds the element in a Rust local with no root
   (`runtime/src/arrops.rs`, `runtime/src/assocops.rs`; §323.4 item 2
   for `sort`). Measured with the `find` probe of the G0b round: the
   interpreter prints `findLast 32`, and the ship tier prints
   `findLast 2055278416`. The literal-lambda path is a script-level
   loop and has roots.
5. The dev tier compares the exit count with the interpreter only. The
   ship tier has no function that reads the count (acceptance 8).
6. `Context.free` and W001: W001 states that the Context grows until a
   collection. That is false in mode 1. Its change is the next section.
