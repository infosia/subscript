<!-- §349 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 349. A Worker has an awaitable receive

*(Added 2026-11-22.)* Origin: the owner decisions of 2026-11-22 and
the measurement in `specs/tracking/worker-receive.md`.

Problem, measured at `7f7be71d`:

1. **The spawning side cannot wait for a message.** `poll()` never
   blocks, and `Inbox.wait()` is for the Worker side only
   (`stdlib.md` §16.1). An async function waits with a loop of
   `poll()` and `await Context.suspend()`.
2. **The loop uses one core.** With one Worker that computes for
   about 180 ms, the loop runs 3.13 million turns, and the process
   uses 363.8 ms of CPU against 174.3 ms for the form with `join()`.
   One turn costs about 55 ns. The Worker takes 5% more wall time.
3. **The loop is the only form that composes with `await`.** The
   form with `join()` blocks the calling function, so a program
   cannot wait for two Workers and do other work.

### 349.1 Rules

Surface (`stdlib.md` §16.1):

```ts
declare class Worker<In extends object, Out extends object> {
  receive(): Promise<Out | null>;  // null = the Worker ended and the outbox is empty
}
```

Terms. A receive **waits** from its call until its handle has a
completion. The **outcome** of a Worker is "runs", "ended", or
"trapped".

1. **`receive()` returns an ordinary handle.** The handle follows
   each rule of a `Promise` value: `await`, `Promise.all`, a field,
   an array, and S013 for a handle that the program drops. The
   `Worker` value keeps its affinity rules (§40).
2. **A message in the outbox completes the receive at the call
   (decision A).** If no earlier receive of that Worker waits and the
   outbox holds a message, the call takes the oldest message, and the
   handle has its completion at the return of the call.
3. **Receives that wait get messages in call sequence (decision
   C).** The oldest message goes to the oldest receive that waits.
4. **A receive that waits completes at a scheduler step of the
   Context.** The step function, and the drain that each driver runs,
   deliver each message that a waiting receive can take, in the
   sequence of rule 3, before the step runs tasks. No other thread
   runs script code or writes to the Context.
5. **An ended Worker with an empty outbox gives `null`.** At the call
   or at a step: if the outcome is "ended" and the outbox is empty,
   each receive that waits completes with `null`. `close()` closes the
   input only; a receive after `close()` gets each message that the
   Worker posts and then `null`.
6. **A trapped Worker with an empty outbox traps the Context, as
   `join()` does.** At the call or at a step: if the outcome is
   "trapped", the outbox is empty, and a receive waits or is called,
   the Context traps with kind 22 at the trap site of the Worker, with
   the message of `join()`. The messages that the Worker posted before
   its trap are delivered first. If the host clears the trap, the next
   step reports it again, as each `join()` does.
7. **While a receive waits, `poll()` returns `null` (decision B).**
   `join()` does not change: it blocks until the Worker ends and does
   not read the outbox. A receive that waited through a `join()`
   completes at the next step.
8. **The queue carries the outcome.** The Worker writes its outcome
   to the state that the outbox shares before the outbox closes. A
   step reads the outcome with no `join()`.
9. **A frame kind of its own.** A receive that waits is a registered
   frame of a new runtime kind, "worker receive". The task visitor of
   the host reports it with a kind number of its own. It counts as
   unfinished work. No host completion function completes it. It is a
   root of a collection (§348 rule 5), and hot reload treats it as it
   treats a host operation.
10. **The pending count shows a receive that can complete.** Each
    Context has one signal that each outbox of its Workers sets at a
    post, at the end of the Worker, and at a trap. While a receive
    waits and the signal is set, the pending count of the Context is
    not zero. A step that delivers nothing clears the signal. So a
    host that steps while the pending count is not zero delivers each
    message. A step with a budget of zero delivers nothing (§168).
11. **A driver with no host step waits on the signal (decision D).**
    The runtime has one function that blocks until the signal is set,
    and returns at once with "nothing to wait for" if no receive
    waits. The C API exports it (`subscript_rt_ctx_async_wait`), and
    `ReloadSession` has the same function (`async_wait`). Each driver
    that owns its loop (the dev JIT entry, the generated `main` of
    the ship tier, `ReloadSession::run_main`) calls it when the
    pending count is zero, and steps again if it returned after a
    wait. A host that owns its loop and has other work does not call
    it, and no other function of the C API or of `ReloadSession`
    blocks on a Worker.
12. **A run that waits for a Worker that does not end does not
    return.** This is the result of `join()` on that Worker today.
    The wait uses no CPU.
13. **The message of a completion is materialized on the thread of
    the Context**, at the call or at the step, by the function that
    `poll()` uses. `receive()` can allocate (§342 rule 10).
14. **The interpreter does not run Workers.** It refuses `receive()`
    as it refuses each Worker operation. Each corpus entry of this
    section states `interpreter: no`.
15. **No hidden cost.** A step locks one outbox for each Worker that
    has a receive that waits, and only if the signal is set. A
    program with no receive pays one test of a count in each step.

16. **The exit count of a program that calls `receive` is not a
    function of the program.** A receive that completes at its call
    adds no outermost exit, and one that waits adds a step, so the
    count follows the time of the Worker thread. The stress sweep
    compares the output and the trap of such an entry, and does not
    compare its exit count (§345 rule 4). The sweep derives the fact
    from the lowered program, and a second derivation from the source
    text must agree. *(Added 2026-11-22, after the corpus entries:
    `a505` gave 6 to 9 exits in eight runs of each tier.)*

### 349.2 Acceptance

1. **The wait uses no CPU.** A program awaits `receive()` of one
   Worker that computes for a measured time. The CPU time of the
   process is under 1.2 times the CPU time of the same Worker with
   `join()`, in the dev JIT and the ship tier. Firing control: the
   loop of the problem, in the same test, is over 1.5 times.
2. **Corpus accept entries**, each with a golden, equal in the dev
   JIT and the ship tier:
   a. `await w.receive()` for each of three messages, then `null`
      after `close()`;
   b. rule 2 and rule 3: messages posted before the calls, two
      receives called before the first `await`, results in call
      sequence;
   c. rule 7: `poll()` gives `null` while a receive waits, and gives
      the next message after that receive completes;
   d. `Promise.all` over the receives of two Workers, and a
      `TaskGroup` whose tasks await receives;
   e. a receive that waits through `join()`.
3. **Corpus trap entry**: a Worker posts one message and then traps.
   The first `await w.receive()` gives the message. The second ends
   the run with kind 22 at the trap site of the Worker, equal in the
   two tiers, and equal to the position that `join()` reports for the
   same Worker (`corpus/trap/t82` is the form).
4. **Corpus reject entries**: a bare `w.receive();` statement (S013),
   and `receive` on a `Worker` in a position that §40 rejects, with
   the header that states what `tsc` does.
5. **Runtime tests**: each of rules 2, 3, 5, 6, 7, 9, and 10 at the
   runtime API, with a firing control for each test that states an
   absence. Rule 10: a loop that steps only while the pending count
   is not zero, plus the wait of rule 11, delivers a message that
   arrives after the first step. Rule 11: the wait returns "nothing
   to wait for" with no receive, and returns after a post from
   another thread; each test has a time bound.
6. **Host header**: the generated header declares the wait function,
   and the kind number of rule 9 is in the visitor documentation.
7. **`tsc`**: the ambient with the member passes the gate.
8. The stress sweep of §342.2 passes: the Worker entries run in the
   two tiers, as the present ones do.
9. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
   passes.

### 349.3 Sections this one amends

- `stdlib.md` §16.1: the surface gets `receive()`, and the text "the
  spawning side never blocks" gets the wait of rule 11 for a driver
  with no host step.
- `collisions.md` Q35: `receive()` has no JavaScript form of the same
  name.
- §168 (the step budget) and the host header: the pending count
  includes a receive that can complete (rule 10).
- §345 rule 4: rule 16 here.

### 349.4 Not in this section

- A callback form (`onMessage`): a step is the only point of
  delivery, so a callback gives nothing that a handle does not.
- A form that posts one message and returns one result.
- An awaitable `Inbox` on the Worker side: the entry of a Worker is
  synchronous (§40).
- `Promise.race`: the language has none.

### 349.5 Phase Review

Form.

- `runtime/src/worker/receive.rs` holds the signal of a Context
  (`ReceiveSignal`: one flag, one mutex, one condition variable), the
  sequence of the receives that wait for each Worker, and the choice
  of rules 2, 3, 5, and 6. A post, a close, and the end of a Worker
  set the signal while they hold the lock of the outbox, so a thread
  that reads a message under that lock reads the signal also. The
  lock sequence is outbox, then signal; no path takes them in the
  other sequence.
- The Worker writes its outcome and closes its outbox in one hold of
  the lock (rule 8). `join()` and a receive report a trap through one
  function, `Context::trap_worker_outcome`.
- A receive that waits is a frame of `RuntimeTask::WorkerReceive`,
  kind 5 for the task visitor. The sequence of a Worker holds one
  count of the frame until the delivery, so a handle that the caller
  releases before the delivery stays valid.
- The delivery is in the drain, inside the script bracket, before the
  tasks run. A step with a budget of zero returns before it.
- `receive` is operation 8 of the Worker intrinsic family. Its result
  is a fresh owned handle, as the result of an async call is; the LIR
  needed no new fact.
- The drivers: the dev JIT entry, the C text of the generated `main`,
  and `ReloadSession::run_main` call the wait. Three loops of tests
  and tools that run a program to its end call
  `ReloadSession::async_wait`.

Measured.

| Item | Result |
|---|---|
| CPU time, ship tier, release build, three runs: receive, `join()`, the loop of the problem | 187.0 to 189.2 ms, 185.5 to 189.7 ms, 377.3 to 395.7 ms |
| CPU time, dev JIT, the same | 221.6 to 222.0 ms, 220.8 to 221.6 ms, 443.5 to 443.8 ms |
| CPU time, debug build, one run | ship tier 208.9, 209.2, 424.7 ms; dev JIT 238.2, 235.2, 464.4 ms |
| The limits of the test | receive under 1.2 times `join()`; the loop over 1.5 times |
| A Worker posts one message and traps: receive form and `join()` form | equal: kind 22, the message of the index trap, the position in the Worker, stdout `got 5`, in the two tiers |
| `a502` to `a506`, 10 runs of each in each tier | 100 of 100 equal to the golden; no run over its time limit |
| Before the drivers called the wait | the program of acceptance 2a printed nothing in the dev JIT and the ship tier |
| At the pin | the checker rejects `await w.receive()` with S018 (measured at `7f7be71d`, which has the code tree of the pin) |
| Stress sweep, 610 entries | dev JIT 607 of 607, ship tier 605 of 605, interpreter 496 of 496; 485 exit counts compared; 5 not compared by rule 16 |
| Exit counts of `a505`, eight runs of each tier | 6 to 9 in each tier |
| Cost of the tests | `codegen/tests/worker_receive.rs`: 1.8 s in a release build, 2.8 s in a debug build; each runtime test under 5 ms, with no sleep |
| `perf-gate --gate`, two passes | each criterion met: `a22` ship tier 1.33x and 1.34x, `collect` ship tier 1.16x and 1.15x, `a22` dev JIT 19.87x and 19.91x, `collect` dev JIT 3.72x and 3.72x |

Phase Review (a subagent with no context): no CRITICAL, 3 MAJOR, 6
MINOR. The review found no lost wakeup, no path where a host-owned
loop blocks, and no message that no root reaches, and it evaluated
each golden by hand.

| Finding | Close |
|---|---|
| MAJOR: the Worker set the signal after it released the lock of the outbox, so a runtime test that read the outbox and then stepped could deliver nothing | the signal is set under the lock |
| MAJOR: the clause "no earlier receive waits" of rule 2 had no test that can fail | `a_receive_waits_at_its_call_while_an_earlier_receive_waits`; Red without the clause |
| MAJOR: the hot reload clause of rule 9 had no test | `a_receive_that_waits_across_a_reload_is_not_stale`; Red without the kind in the stale test |
| After the host cleared a rule 6 trap, the receive did not complete and did not trap again | a trap exit of the delivery sets the signal again; rule 6 states it |
| A message was lost if its materialize step trapped at a step | the delivery gives the record back to the outbox; a test with an allocation limit |
| An unread message of a released handle stays until a collection | open item 1 |
| The three tutorials, the language reference, and one header text stated the former pending count or did not state `receive` | each is updated |
| The margins of the CPU test in a debug build | measured: the ratios are 1.0 and 2.0 in a debug build also |
| Rule 16 leaves out two entries whose count is a function of the program (`a504`, `a506`) | open item 3 |

The first run of the full gate on this section failed on one test:
`reload_mode_reproduces_every_committed_golden` ran `a503` and `a505`
with a loop on the pending count and ended while a receive waited.
`ReloadSession` had no wait function. Rule 11 now states it, and the
run after that:

```text
gate full c7ca4b37dc52fffe68a3441b71ec50b38ad12e88 dirty:62 debug 4004/0/5 release 3985/0/5 skips 2/0 clippy 4/10/13 goldens-moved 1 exit 0
```

`tools/hygiene.sh`: exit 0.

### 349.6 Open

1. A message of a receive that completed, whose handle the program
   releases with no `await`, is not released at that point. A
   collection reclaims it. With automatic collection off and no
   explicit collection it stays until the Context ends. S013 rejects
   a handle that the program drops, so the shape needs a handle that
   the program holds and does not await.
2. A receive that completes at its call loses its message if the
   materialize step traps there (an allocation failure). The path at
   a step keeps it.
3. Rule 16 leaves out the exit count of each program that calls
   `receive`, also where the count is a function of the program
   (`a504`, `a506`).
4. The CPU test of acceptance 1 is for unix hosts only: the
   repository has no portable read of the CPU time of a child. On
   Windows the acceptance has no test.
5. A run that waits for a Worker that does not end and does not post
   does not return (rule 12).
6. Rule 14 (the interpreter refuses `receive`) and rule 15 (the cost
   with no receive) have no test of their own: the corpus headers
   state the first, and `perf-gate` reads the second.
7. Closed by §350. At the landing of §349 the language had no
   `Promise.race`, so a program could not take the first message of
   some Workers with one `await`.

