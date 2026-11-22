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
   its trap are delivered first.
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
    waits. The C API exports it. Each driver that owns its loop (the
    dev JIT entry, the generated `main` of the ship tier,
    `ReloadSession::run_main`, the capture binary) calls it when the
    pending count is zero, and steps again if it returned after a
    wait. A host that owns its loop does not call it, and no other
    function of the C API blocks on a Worker.
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

### 349.4 Not in this section

- A callback form (`onMessage`): a step is the only point of
  delivery, so a callback gives nothing that a handle does not.
- A form that posts one message and returns one result.
- An awaitable `Inbox` on the Worker side: the entry of a Worker is
  synchronous (§40).
- `Promise.race`: the language has none.
