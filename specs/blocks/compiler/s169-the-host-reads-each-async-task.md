<!-- §169 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 169. The host reads each async task

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 6, second part. The owner selected it on 2026-10-06.

Problem: the host reads three counts (`subscript_rt_ctx_async_pending`,
`subscript_rt_ctx_async_unfinished`, and the §168 report). A count does
not identify a task, its state, or the task it waits for. When
`pending` is 0 and `unfinished` is not 0, work is blocked (§94.2), and
no host API names the blocked tasks or the wait between them. Measured
at `16e562c2`: `runtime/include/subscript_runtime.h` declares no per-task
read. A frame header holds the position id of its allocation, and the
host resolves a position id through `subscript_alloc_positions`
(`program.alloc.h`), but no API gives a frame to the host.

### 169.1 Rules

1. The runtime adds this host API:

   ```c
   typedef struct subscript_rt_async_task_info {
       uint64_t task_id;
       uint64_t awaited_task_id;
       uint32_t state;
       uint32_t kind;
       uint32_t function_pos_id;
       uint32_t await_pos_id;
       uint32_t create_pos_id;
       uint32_t reserved;
   } subscript_rt_async_task_info;

   typedef void (*subscript_rt_async_task_visitor)(
       void* userdata, const subscript_rt_async_task_info* info);

   uint64_t subscript_rt_ctx_visit_async_tasks(
       const subscript_rt_context* ctx,
       subscript_rt_async_task_visitor visitor, void* userdata);
   ```

   The struct has no padding. The C host test checks each field offset
   with `offsetof` against the Rust layout. The call gives each registered task to
   the visitor once, in `task_id` order, and returns the number of
   tasks. A null visitor gives 0. The call runs no script, allocates no
   Context memory, and changes no state.
2. `task_id` is assigned at registration. The ids of one Context start
   at 1, increase by 1, and are never used again in that Context. 0 means
   "no task". Because each tier registers in the same order, the ids of
   one program are the same in each tier.
3. `kind` is 1 for an invocation and 2 for an aggregate (§166).
4. `state` is one of these values:
   - 1 `READY`: in the ready queue, including a trapping job at the
     ready head (§94.2).
   - 2 `PARKED`: on the parked list, waiting for the next checkpoint
     (`Context.suspend()`).
   - 3 `WAITING`: registered as a waiter on another task's handle.
   - 4 `ACTIVE`: running now (the visit is from a host function that
     script code called).
   - 5 `COMPLETE`: has a cached completion; a holder keeps it.
   - 6 `STOPPED`: stopped at host clearance of a trap (§94.2).
   An aggregate that has not completed is `WAITING`.
   The states are total: each registered task is in exactly one state.
   A task that traps before its first suspension (in its call prefix,
   or in a host kick of an export) enters the stopped set when it traps
   and reads `STOPPED`. It has no resume state, so it never runs again
   (§94.2). The visit has no fallback state; a test builds each path
   into each state.
5. `awaited_task_id` is the id of the task whose handle a `WAITING`
   invocation waits on. It is 0 for every other state and for an
   aggregate. The call finds it from the waiter lists at visit time; the
   scheduler stores no extra fact for it.
6. `function_pos_id` is the position id of the task's async function or
   async arrow: the position id that the frame header holds. For an
   aggregate it is the position of the `Promise.all` call. The host
   resolves it through `subscript_alloc_positions`.
7. `await_pos_id` is the position of the `await` or `Context.suspend()`
   where the task is suspended. It is 0 for `READY` before the first
   resume, `ACTIVE`, `COMPLETE`, and for an aggregate. The generated code
   gives this position to the runtime at each suspension. The position
   id 0 means "no script site" (§112).
8. The API does not report payloads, results, exceptions, or pointers.
   The runtime keeps no history of past states.
9. A call that creates a task runs the callee's prefix through one
   runtime call,
   `uint8_t subscript_rt_async_start(subscript_rt_context* ctx, uint8_t* frame, uint8_t* out, uint32_t pos_id)`,
   which returns the done flag of the prefix. It is in the generated-code
   ABI, not in the host header. The call records `pos_id`, the position
   of the call, as `create_pos_id`; it puts the frame in the active set
   during the prefix (`ACTIVE`); and it records a prefix trap (rule 4).
   This is one runtime call more for each async call. Round 3 measured
   the call without the position: `settled-awaits` 1.036, `held-handles`
   0.963, `deep-chains` 1.000 against the pin. `create_pos_id` is 0 for a
   host kick and for an aggregate. `reserved` is 0. For an awaited call
   of a declared callee (`await f()` where `f` is an async function
   declaration, `await recv.m()`), the HIR and the LIR hold one
   position, the `await` expression, which contains the call; that
   position is `create_pos_id`. A call through a function value (§167)
   records the position of the call.
10. `ReloadSession` has the same read in Rust
    (`async_tasks() -> Vec<AsyncTaskInfo>`), with the same fields. The
    interpreter has the same read, so that the differential tests
    compare the three tiers.
11. The host header, the C and C++ tutorial, and the Rust tutorial state
    the API, the states, and how to name a blocked task: visit the
    tasks when `pending` is 0 and `unfinished` is not 0, then follow
    `awaited_task_id`.
12. A position id is an index into the position table of one tier. Each
    tier builds its table from the sites that its own lowering visits,
    so the id of one site can differ between tiers (measured at
    `049a0d6b`: one async function has id 7 in the dev JIT and 6 in C
    AOT). The fact that the tiers share is the resolved position (file,
    line, column). `ReloadSession` gives the resolved position of each
    position field beside its id. The interpreter has no table, and
    reports the resolved position only. In one `ReloadSession`, the
    position table of a new generation extends the table of the previous
    generation, so a position id keeps one meaning for the whole session.
    Code that a reload keeps (an async arrow or a lambda from an earlier
    generation, §167 rule 10) gives ids that the current table resolves.
    The same holds for a trap position. The Phase Review measured that a
    per-generation table resolves an id of kept code to a wrong site.

### 169.2 Acceptance

1. A runtime test for each state of rule 4 and each kind of rule 3, with
   a same-shape control for each state change. A test of a wait ring
   (each task awaits the next through a module array of handles:
   `const ring: Promise<void>[] = [];`, `await ring[next];`) shows `pending`
   0, `unfinished` not 0, and each `awaited_task_id` names the next task
   of the ring.
2. A test that the visit runs no script and changes no count: the counts
   and the live allocation count are equal before and after the visit.
3. A differential test runs the same programs in the dev JIT
   (`ReloadSession`), the C AOT tier (a C host that calls the new API
   through the generated header and resolves positions through
   `program.alloc.h`), and the interpreter. After each budgeted
   checkpoint (§168, budget 1), each tier gives the same task records,
   with each position field compared as its resolved position (rule 12).
   The C host builds once per program.
4. The async-cost benchmark (`benchmarks/src/bin/async-cost.rs`) on the
   existing workloads: the median is at most 1.05 times the pin median
   (release, best of three). The rule 7 position store is on each
   suspension, so this measurement covers it.
5. No `.expected` golden moves. The host header test passes with the
   new declarations.

### 169.3 Open

The Phase Reviews found these. None is CRITICAL or MAJOR.

1. A host visitor that calls a step or another mutating API from inside
   the callback is not guarded; the visit holds metadata borrows across
   the callback (contrived).
2. The C emitter no longer returns an internal error for an invalid
   async call target kind at a suspension; only the LIR verifier
   rejects it.
3. No program reads `ACTIVE` from a host function under generated code
   in the three-tier test; a hand-written resume tests it. The trapping
   `READY` head is not in the three-tier test.
4. The runtime ring control in `runtime/tests/async_inspection.rs` is
   not same-shape: it builds completed frames that also wait. The
   script ring in the three-tier test meets acceptance 1.
5. The runtime visitor is O(n²) in live tasks (queue `contains` and a
   waiter scan per task). The interpreter prunes its whole task registry
   on each registration.
6. The interpreter keeps task ids, its task registry, and the park
   position in builds without tests, but only the `cfg(test)` read uses
   them (`cfg` scope convention).
7. The three-tier test does five optimized C builds and states no cost
   (core principle 15).
8. A visit that meets an unclassified frame returns 0 records; the host
   cannot tell it from no tasks. No good-faith path reaches it
   (contrived).
9. The session position table grows by the sites of each accepted
   reload, with no dedup; the cost is not stated. The doc comments of
   `ModLower::pos_id` and `Lowered::positions` state that the first id
   is 1, which is false in a reload generation.
10. No test reads `async_tasks` or a trap position after a rejected
    reload.
