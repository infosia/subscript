<!-- §94 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 94. Host-driven async continuation queue

*(Contract decision 2026-09-08, Codex as the owner-assigned
orchestrator. Landed 2026-09-09.)*

Baseline pin: `d8cc19c34ac5e2db27bdc0411d5210924528029b`.
B1 and B2 measured both production tiers at that pin. The permanent
record is `specs/tracking/s94-async-continuations.md`.

The problem is observable order. Two called bodies start together,
but their continuations currently depend on the holder's await order.
A completed await currently runs inline. This contract removes those
two dependencies while the host retains control of checkpoints.

This section supersedes §26's root polling, §70's execution rationale,
and §92's rules 1a, 1b, 2, 5, and 7 where they conflict.
It preserves call-time start, the accepted type surface, and generator
semantics. It adds no Promise construction, combinators, thenables,
rejection handling, implicit collection, or background execution.

### 94.1 Execution protocol

1. An async call runs its body synchronously to its first await or
   return. A body without await completes before the call returns.
2. Every await suspends the caller. This includes completed handles,
   direct calls, held handles, methods, and generic instances.
3. `SuspendKind::Async` registers the frame in the parked list.
   `Context.suspend()` remains the existing primitive await form.
4. `SuspendKind::AsyncCall` creates and starts the child, then registers
   the caller on that child's completion. `AsyncHandle` registers on
   the named handle. Neither await path resumes the child.
5. An unfinished handle holds waiting continuations in registration
   order. Completion moves them to the ready queue's tail in that order.
6. An await of a completed handle appends the caller to the ready tail.
   It does not invoke the continuation inline.
7. Calls and export kicks never drain ready work. A suspension registers
   before control returns to its caller or host.
8. One checkpoint appends the entire pre-existing parked list after
   existing ready jobs. It then drains the ready queue in FIFO order.
   Jobs added during that drain participate in the same checkpoint.
9. A frame newly parked during the drain waits for the next checkpoint.
   A frame can execute multiple distinct continuations in one checkpoint.
   It has only one outstanding scheduler registration at a time,
   apart from its current active resume during registration transfer.
10. Exported roots and call-created frames use the same rules. Calls
    retain source evaluation order. Export kicks retain host order.
11. A resumed await reads the immutable cached completion and binds
    the successor's result parameter. It never polls the awaited body.
    Multiple holders and repeated awaits read the same completion.
12. A checkpoint has no job budget or termination guarantee. A finite
    ready chain drains in one checkpoint. An infinite ready chain can
    prevent return. `Context.suspend()` is the explicit host-step boundary.
    This is not a real-time execution bound.

The LIR instruction and terminator definitions must state this protocol.
The verifier checks async target kind, result types, suspension edges,
and required stale-trap positions. Scheduling readiness is a runtime
invariant, not a fact a static verifier can prove for arbitrary values.
A resume without its required completion is an internal protocol defect.
No consumer silently re-registers, polls, or fabricates a result there.
Use the existing `TrapKind::Internal` (runtime code 11), with message
`async resume without completion`, at the suspension position.
The Context stops under the ordinary trap policy. The interpreter
reports the same internal defect. Add a focused invalid-protocol test;
this is not a new source-language trap or a library panic.

### 94.2 Host API, traps, and lifetime

`async_pending` returns ready jobs plus parked registrations, including
a trapping job that the Context still holds trapped. It excludes blocked
continuations, stopped frames, and completed frames retained only by
handles. `async_step` returns this count.

`subscript_rt_ctx_async_unfinished` returns the number of registered
async invocations without a cached completion. `ReloadSession` exposes
an equivalent read-only accessor. Both observers execute no script.
Other B1 counters remain test or benchmark instrumentation.

Pending zero means no work can advance at a checkpoint. It does not
promise successful completion. An unfinished positive count at that
boundary exposes blocked work, including self-await and mutual waits.
No deadlock detector, new diagnostic, or cancellation API is added.
Standard runners stop at that boundary as §26.3 specifies.

A trap before the first await reports during the call. A trap after
an await reports when its continuation executes, with its original
callee position. It can therefore report earlier or later than at
the baseline. Traps remain Context traps, not Promise rejections.

A trapped checkpoint stops immediately and preserves the trapping
registration and all other outstanding work. Repeated steps are no-ops
until host clearance. Clearing the trap does not clear frame staleness.
A stale async resume traps at the suspension position before body effects.
The JIT retains old code while queued frames can reference it.

*(Added 2026-09-09, after the Phase Review measured a replay.)* **A
trap inside a resumed continuation stops that frame at host
clearance.** The transition has two steps, and the order matters.

While the Context stays trapped, nothing changes. The scheduler
records the trapping frame and keeps it at the head of the ready
queue, so `async_pending` counts it and repeated steps are no-ops, as
the paragraph above states.

`clear_trap` performs the transition. The scheduler moves the recorded
frame out of the ready queue into the stopped set. From that point the
frame never re-enters the ready queue, and a later checkpoint advances
every other registration. Its registration stays until Context
release. `async_pending` excludes it, because nothing can advance it.
`async_unfinished` includes it, because it holds no cached completion.
Waiters on its handle stay blocked, and `async_unfinished` reports
them.

The reason is the saved state. A frame stores the state word of the
suspension it resumes from, and it writes the next state word only when
it suspends again. A trap between those two points leaves the word
naming the earlier suspension, so a second resume repeats every effect
between that suspension and the trap. The review measured the replay:
a body that prints, awaits `Context.suspend()`, prints again, and then
traps on an out-of-range index re-printed its second line and leaked
one allocation on each cleared step, without advancing.

The dev-tier reload staleness trap is the one exception. It reports at
the adapter head before any body effect, so the frame is unchanged.
`clear_trap` leaves that frame in the ready queue, and stepping again
reports the staleness again, as §18.2b has it. The scheduler decides
by the recorded trap kind. Every other trap stops the frame.

A registration owns a reference independent of caller handles. That
ownership transfers between ready, parked, blocked, and active states.
A new suspension acquires its registration before the active resume
releases its ownership. Normal completion releases active scheduler
ownership immediately. No extra cleanup checkpoint counts as pending.

The completion cache and values reachable from it survive until their
last owner releases them. Collection roots include all scheduler states,
active frames, completion values, and blocked continuations.
Context release discards all work without resumption. It releases
scheduler storage, including blocked cycles, without implicit collection.
No cleanup body is guaranteed during Context teardown.

The generated-code registration ABI carries the fulfilled-value size
from the LIR return type. Runtime result storage has the required
alignment and size. Runtime and generated code read the contracted
resume pointer at frame offset 8 (§70.2). All ABI declarations and
bindings change together; the host header is generated from its source.

### 94.3 Corpus and semantic exit criteria

New ordering entries must be Red against a binary built from the pin.
Expected order derives from this contract before implementation.
Node is a divergence detector for comparable shapes, never the oracle.
New acceptance cases add no syntax and require no new rejection rule.
Existing rejected Promise forms and required-await diagnostics remain.

The implementation must supply these cases:

- Two nested settled chains and a held outer handle (B1 p15 and a185).
- Two parents awaiting one handle, followed by repeated completed awaits
  (B1 p14). Include reference and aggregate completion values.
- Concurrent children across parent suspension (B1 p03 and p04).
- Mixed ready and parked work in both creation orders (B2 q01 and q02).
- A trap after settled await, with caller effects before the trap (B1 p08b).
- Direct, held, method, and generic await paths under the same protocol.

Use the next free corpus IDs. Add direct host tests for exact checkpoint
boundaries, quiescence counts, and the unfinished observer. A finite long
settled chain finishes in one checkpoint. A self-await control reaches
pending zero with unfinished work and releases its Context safely.

*(Added 2026-09-09 by the Phase Review.)* Three more host tests, each
through generated code and a real Context, not a synthetic frame:

- **A cleared trap does not replay.** A body prints, awaits
  `Context.suspend()`, prints again, then traps. The host clears the
  trap and steps five times. The second print appears once. The
  allocation count does not grow across those steps, and pending
  reaches zero while unfinished stays positive.
- **A cleared trap in a started callee does not replay either**, and it
  leaks no registration: `unfinished` does not grow across cleared
  steps.
- **Teardown runs no continuation, and the test can fail.** The existing
  Context-level test holds a counter that a resumed continuation would
  increment. The `ReloadSession` test must observe the same fact after
  the drop, not only before it.

The self-await control is a host test with the `unfinished` observer,
not an interpreter test. A mutual two-frame wait does not stand in for
it: self-registration appends a frame to its own waiter list, which is
a distinct path.

Carry B2's ready and parked reload, trap-retry, and blocked-parent tests
into permanent tests. Assert exact positions from independent source
locations, not only equality between repeated reports. Keep the mixed
queue mutation as recorded sensitivity evidence, not production code.

Test ownership without print allocations, pending Context teardown,
explicit collection during a drain, and cached owned-result survival.
All new public observers have direct unit tests.

The independent interpreter implements §68.7.4 and §94 from their
protocol, with ready, parked, and blocked states. It must not call the
production scheduler or reuse either tier's driver as its oracle.
All admitted corpus cases follow the same order in all three witnesses.

Authorized existing output changes are a154, a155, and a185, whose exact
before/after bytes appear in the tracking record. No existing trap
expected file changes. Any additional change requires evidence and
orchestrator review before its expected file moves.

The aggregate LIR snapshot can change only for async instruction streams
and new corpus entries. Record byte sizes and prove unrelated blocks
identical. a185 becomes `js-comparable: yes`; C16 references retire.
Update generated reference sources, generated outputs, and tutorial prose.
Do not hand-edit generated files or invent a new divergence to hide order.

### 94.4 Cost and final validation

The implementation acceptance limit is 3.0 times the baseline median
for each B1 workload: 200,000 completed-handle awaits; 20 rounds of
2,000 held handles; and 400 chains of depth 200.

This limit is set after B1's measurements and before permanent
implementation. It is a regression cap, not an independent prediction
or an unavoidable cost of JS order. It applies to ship C on aarch64
macOS, the measured platform. Other platforms report results without
an invented numeric threshold. The existing §3 limits still apply.

Use identical sources, release runtime, compiler options, and host
harness on both revisions, with a fresh Context per iteration.
Measure initialization, export calls, all checkpoints, and Context
release. Exclude code compilation. Use three warmups and eleven timed
iterations, then report median and range. If spread exceeds 20%, repeat
under quiet conditions before judging the ratio. Build the baseline
from the pin in isolated scratch storage; never change the active checkout.

Preserve reproducible workload sources and a benchmark driver in the
repository. Record old/new timing and Context payload counters separately
from Rust scheduler allocations. Do not infer total retained memory from
Context counters. Benchmark instrumentation does not become a public API.

Move the LIR form first, then one consumer at a time. Record expected
migration mismatches without changing the final exit criteria.
Final acceptance requires `tools/gate.sh full` exit 0, both profiles,
full release interpreter coverage, the unchanged clippy ceiling 7/18/13,
stock `tsc`, rustfmt, §3 benchmarks, and this section's async measurements.

The phase ends with an independent fresh-context review and correction
of every CRITICAL or MAJOR finding. Run `tools/hygiene.sh` afterward.
The coding agent leaves a reviewable diff and REPORT; it does not commit.
