<!-- §39 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 39. Workers round 2 — runtime threads, channels, worker lifecycle

Owner decision 2026-08-02: Workers are **standard library**, not a
host pattern; this supersedes the CLAUDE.md sentence listing
threads among host-only capabilities (the CLAUDE.md revision and
the Q35 register entry land with round 3). Round 2 is runtime-only:
no language surface, no prelude change, no corpus change. The
round-3 script surface was probed before this contract: the
Worker/Inbox/Outbox ambient and two probe programs — including
`Worker.spawn(entry)` type-argument inference — type-check under
stock `tsc` (exit 0, recorded in tracking).

### 39.1 Model

A worker is a runtime-owned OS thread running a dedicated Context
of the same program image (§38 isolation). The spawning generated
code supplies two C function pointers: the program's module
initializer and the worker entry. The runtime creates the worker's
Context **on the worker thread**, runs the initializer, runs the
entry with the Context and its channel endpoints, and releases the
Context on that same thread — §38.1 thread-affinity holds by
construction. Messages cross as byte copies of fixed-size payloads
through two runtime-owned queues per worker (parent→worker,
worker→parent); the receiving side materializes each message as a
fresh allocation in the receiving Context (invariant 1 makes the
byte copy tier-portable). Queues are unbounded; posting never
blocks. Workers may spawn workers.

### 39.2 C API

New `subscript_rt_*` functions covering: spawn (parent Context,
initializer pointer, entry pointer, in/out payload sizes), post to
a worker, non-blocking poll from a worker, close (subsequent worker
receives observe end-of-input), join, and the worker-side endpoint
operations (blocking wait, non-blocking poll, post). Exact names
and signatures are the implementer's within these semantics; the
generated host header documents the public subset, and
generated-code-only entry points stay out of it (the
`subscript_rt_globals_init` precedent).

Failure: a trap in the worker trap-stops its entry per C6 and the
thread then ends normally. `join` on a worker whose Context trapped
**traps the joining Context** — trap kind `worker-trapped` (22) — a
worker failure is loud at the join point, never silent. `close`
then `join` is the orderly shutdown; a worker handle never outlives
its parent Context (release of the parent closes, joins, and frees
remaining workers — teardown may discard queued messages, the
§26.2 no-cleanup precedent).

### 39.3 Concurrency contract

The worker/channel module is the runtime's only shared-mutable
state (tracking records the pre-existing zero-global finding).
Blocking waits use OS synchronization (condvar), never spinning.
Every `unsafe impl Send`/`Sync` carries a `// SAFETY:` comment.
§14.6 is unchanged: each Context, worker Contexts included, is
driven only from its own thread.

### 39.4 Exit criteria (pre-registered)

1. Runtime unit tests drive a hand-written echo worker through the
   C ABI (the `test_async_resume` precedent): spawn, post N,
   worker waits and replies N, parent polls N, close, join —
   deterministic, headless.
2. A trap raised in the worker entry surfaces as trap kind 22 on
   join in the parent (unit test); a clean worker joins without
   trapping.
3. Two workers concurrently, with interleaving-independent
   assertions only (per-worker reply sets, never global order).
4. Parent-Context release with live workers closes, joins, and
   frees them (unit test; no leak under the existing allocation
   accounting).
5. Full gate green; no golden moves; generated-header byte-compare
   green; no new mutable global state outside the worker/channel
   module.
