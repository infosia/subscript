<!-- §168 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 168. A checkpoint with a dispatch budget

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 6. The owner selected it on 2026-10-06. Task inspection
(item 6, second part) is not in this section.

Problem: one checkpoint drains the ready queue to empty (§94.1 rule 12).
A host that calls it once per frame cannot bound the script work in that
frame. Measured at `726f2678` (`async-cost`, release, aarch64/macOS):
the `settled-awaits` workload (200,000 awaits of a completed handle)
runs in one checkpoint, median 31.2 ms. That is more than one 16.7 ms
frame at 60 Hz. The host has no API that returns before the queue is
empty.

### 168.1 Rules

1. The runtime adds this host API beside `subscript_rt_ctx_async_step`:

   ```c
   typedef struct subscript_rt_async_step_report {
       uint64_t dispatched;
       uint64_t pending;
       uint64_t unfinished;
       uint64_t budget_exhausted;
   } subscript_rt_async_step_report;

   subscript_rt_async_step_report subscript_rt_ctx_async_step_budget(
       subscript_rt_context* ctx, uint64_t max_dispatches);
   ```

   The struct has no padding. `subscript_rt_ctx_async_step` keeps its
   signature and its behaviour.
2. A dispatch is one ready job that the checkpoint starts: an invocation
   continuation, or an aggregate reaction (§166 rule 14).
3. If `max_dispatches` is greater than zero, the call does §94.1 rule 8
   at entry: it appends the pre-existing parked list after the ready
   jobs. It then starts ready jobs in FIFO order until the queue is
   empty, or until it started `max_dispatches` jobs. Jobs that the drain
   adds join the same queue, as in rule 8.
4. When the budget stops the drain, the remaining ready jobs keep their
   order. The next checkpoint, budgeted or not, starts them first. A
   frame parked during the call waits for the next checkpoint (§94.1
   rule 9).
5. If `max_dispatches` is zero, the call starts no job, appends no parked
   frame, and runs no script. It returns the counts.
6. One dispatch runs to its next suspension, its completion, or a trap.
   The budget does not bound the time of that segment. The API is not a
   time limit, and no text states that it is.
7. The report: `dispatched` is the number of jobs the call started,
   including a job that trapped. `pending` equals
   `subscript_rt_ctx_async_pending` at return. `unfinished` equals
   `subscript_rt_ctx_async_unfinished` at return. `budget_exhausted` is
   1 when the call started `max_dispatches` jobs and the ready queue
   still holds a job; else it is 0.
8. A trap stops the call as in §94.2. The trapping job stays at the
   ready head. On a trapped Context the call is a no-op: `dispatched` is
   0, and the counts are current.
9. `ReloadSession` has the same operation in Rust
   (`async_step_budget(max) -> Result<AsyncStepReport, RunError>`), with
   the same fields. The interpreter has the same budgeted step, so that
   the differential tests compare the three tiers.
10. The unbudgeted step and the budgeted step use one drain in each tier.
    The unbudgeted step is the budgeted drain with no limit.
11. The host header (`runtime/include/subscript_runtime.h`), the C and
    C++ tutorial, and the Rust tutorial state the API, rule 6, and the
    loop form: call the budgeted step each frame; work remains while
    `pending` is not zero.
12. §94.1 rule 12 stays true of `subscript_rt_ctx_async_step`. Its text
    names §168 as the bounded form.

### 168.2 Acceptance

1. A runtime test: a chain of N completed awaits with budget k returns
   after exactly k dispatches with `budget_exhausted` 1. Repeated calls
   finish the chain. The output and the job order equal one unbudgeted
   step. A same-shape control with a budget larger than the work
   returns `budget_exhausted` 0.
2. A test for each of rules 4, 5, and 8: a frame parked during a
   budgeted call does not run in that call; a zero budget runs no script
   and promotes no parked frame; a trap reports `dispatched` with the
   trapping job and keeps it at the ready head.
3. An explicit collection between two budgeted calls keeps every ready
   job and its frame alive.
4. A differential test runs the same programs in the dev JIT
   (`ReloadSession`), the C AOT tier (a C host that calls the new API
   through the generated header), and the interpreter, with budgets 1,
   3, and unbounded. Each tier gives the same output, the same
   `dispatched` sequence, and the same final counts.
5. The async-cost benchmark (`benchmarks/src/bin/async-cost.rs`) on the
   existing workloads: the median is at most 1.05 times the pin median
   (release, best of three). The benchmark also records the
   `settled-awaits` workload with a budget of 1,000 dispatches per call:
   the call count and the maximum time of one call.
6. No `.expected` golden moves. The host header test passes with the new
   declaration.

### 168.3 Open

The Phase Review found these. None is CRITICAL or MAJOR.

1. The aggregate trap test in `runtime/tests/async_budget.rs` checks
   that clearance gives no second trap, but not that the aggregate frame
   is freed. A clearance that left the frame would pass.
2. The differential test in `codegen/src/interpreter/budget_tests.rs`
   builds the C program once for each budget, six C builds in all. Only
   the budget changes. A budget in `argv` needs one build per program
   (core principle 15).
3. The interpreter counts `unfinished` by a walk from its handle table
   and queues, not from a registry. A wait ring whose handles left the
   table is not counted (contrived).
4. An aggregate reaction that traps through the missing-completion path
   after it takes its input keeps one count on that input after
   clearance (contrived; only a corrupt registration state reaches it).
