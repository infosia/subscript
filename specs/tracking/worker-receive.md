# `Worker.receive()` — an awaitable receive on the spawning side

Status: **measured 2026-11-22 at `7f7be71d`; the contract is
`compiler.md` §349.** The measurement round changed no file of the
tree.

## Owner decisions, 2026-11-22

- The method is `receive(): Promise<Out | null>`. `null` states that
  the Worker ended and its outbox is empty. A callback form and a
  one-result form are not in this change.
- If the Worker Context trapped, the receive traps, as `join()` does.
- A. If a message is in the outbox at the call, and no earlier receive
  waits, the handle completes at the call.
- B. While a receive waits, `poll()` returns `null`.
- C. Receives that wait get the messages in call sequence.
- D. In a run with no host step, the drain blocks on the Worker
  queues when no task can run and a receive waits. A host that calls
  the step function never blocks.

## The cost of the present form

The present form is a loop: `poll()`, then `await Context.suspend()`
if the result is `null`. One Worker computes for about 180 ms (ship
tier) or 210 ms (dev JIT). Each figure is the mean of three runs after
one warm-up run, on arm64 macOS with 8 CPUs.

| Tier | Program | Wall, ms | User and system, ms | Turns of the loop |
|---|---|---|---|---|
| ship | one Worker, `join()` then `poll()` | 175.4 | 174.3 | |
| ship | one Worker, the loop | 184.2 | 363.8 | 3.13 M |
| ship | four Workers, `join()` | 193.8 | 755.5 | |
| ship | four Workers, four loops in `Promise.all` | 202.0 | 989.5 | 4.39 M |
| dev JIT | one Worker, `join()` then `poll()` | 215.7 | 213.8 | |
| dev JIT | one Worker, the loop | 227.6 | 443.1 | 3.78 M |
| dev JIT | four Workers, `join()` | 235.6 | 908.5 | |
| dev JIT | four Workers, four loops in `Promise.all` | 238.3 | 1,135.5 | 4.85 M |

- The spawning thread uses one full core while it waits: the CPU time
  over the `join()` form is 189 ms against 184 ms of wall time (ship
  tier, one Worker).
- One turn costs about 55 ns in each tier.
- The Worker is slower with the loop: 5% more wall time with one
  Worker.

## What a run does today with a task that waits

- Each driver loops on the pending count and stops at zero: the dev
  JIT entry (`codegen/src/jit/entry.rs`), the generated `main` of the
  ship tier (`codegen/src/ship.rs`), `ReloadSession::run_main`
  (`codegen/src/reload.rs`), and the golden capture binary. No driver
  reads the unfinished count.
- A run with tasks that wait and no pending task exits with code 0 and
  no message, in the dev JIT and the ship tier.
- The interpreter refuses each Worker operation
  (`codegen/src/interpreter/dispatch.rs`), so each Worker corpus entry
  states `interpreter: no`.

## Facts that shape the contract

1. `join()` reports the trap of the Worker at the trap site of the
   Worker, not at the `join()` (`corpus/trap/t82`). A receive follows
   that rule, so no completion holds a trap until an `await`.
2. The outcome of a Worker is only in the result of its thread today,
   and the outbox closes after the outcome is known. A step cannot
   read the outcome without a `join()`. The queue must carry it.
3. A receive that waits is not a pending task, as a host operation is
   not. A host that steps only while the pending count is not zero
   does not deliver a message that arrived. The pending count must
   show it.
4. Each queue has a condition variable of its own, and the spawning
   side waits on none today. A wait on some Workers needs one signal
   for each Context.
5. A Worker is not released at the end of a scope: the Context holds
   it until the Context ends. So a receive that waits holds a valid
   Worker.
6. A host operation frame cannot serve as it is: the host completes
   it by an endpoint id, and its result kinds have no message object.
7. `Promise<Out | null>` as the result of an async function passes
   `await`, `Promise.all` over a literal and over an array, and a
   `TaskGroup` task, in the checker and in a dev JIT run. The language
   has no `Promise.race`.
8. A bare call statement of a function that returns a handle is S013.
9. `tsc` accepts the ambient with the new member (exit 0), and reports
   TS2339 at each use with the present ambient.
10. A run where the Worker does not end and does not post blocks with
    decision D. The same program uses one core without end today, and
    a `join()` of that Worker also does not return.
