<!-- §177 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 177. Bookkeeping cost stays linear

*(Added 2026-10-07.)* Origin: `compiler.md` §172.3 item 1, §160.3
item 1, and §169.3 item 5. On 2026-10-07 the owner selected the three
items. The measurement round at `9767cd18` is
`specs/tracking/s177-cost-measurement.md`.

Problem: two lookups rebuild or rescan a whole table per element, and
one costs every program that the interpreter or the LIR builder
prepares.

| Site | Measured cost | Cause |
|---|---|---|
| LIR builder, each call instruction | Interpreter preparation of a fixed 265-entry set: 0.182 s at `9647c72a`, 0.305 s at HEAD (+67 %); the operation table is built 13,055 times | `FunctionBuilder::emit` calls `intrinsic_operations()` for each call: it formats every operation name and builds every row |
| Interpreter, each task registration | n tasks live at one time: 0.005 s, 0.113 s, 17.08 s for n = 1,000, 10,000, 100,000 | `register_task_id` runs `retain` over the whole task registry |
| Runtime task visitor, one visit | 0.00055 s, 0.085 s, 10.37 s for n = 1,000, 10,000, 100,000 waiting tasks | `visit_async_tasks` scans each queue and every waiter list for each task |

The interpreter row reaches an ordinary async program that holds many
tasks; it needs no host call. The dev JIT and C AOT run the same
program in 0.035 s and 0.016 s at n = 100,000.

§160.3 item 1 does not reproduce at its recorded size: the annotated
field forms check in 1.056 (reads) and 1.042 (constants) times the
`40b0cbff` time. The excess is spread over several checker scans, and
no measured prototype removes the read excess. It stays open (177.3).

### 177.1 Rules

1. **One operation table.** The intrinsic operation table is built
   once per process and is immutable. Each lookup by family and
   operation borrows it and costs O(1): an index, not a linear search.
   This applies to the LIR builder and to the LIR verifier.
   Each LIR module keeps its own copy of the rows that it records
   (`intrinsic_operations`), built from that table.
2. **Registration does not scan the registry.** Collection releases
   the unreachable task handles in increasing task id (§172, `t98`), so
   the interpreter task ids exist in every build. Only the `cfg(test)`
   task read uses the task registry (§169.3 item 6). The registry and
   its sweep state are declared inside `cfg(test)` (the `cfg` scope
   convention), so a build without tests keeps no registry. In a test
   build, the interpreter removes a registry entry when the last owner
   releases a completed task. A sweep of dead weak entries runs only
   when the registry length reaches twice its length after the last
   sweep, and at least 64. The sweep removes each other dead entry.
   Registration costs amortized O(1). The registry holds at most twice
   the live tasks at the last sweep, plus the tasks registered after
   it.
3. **One visit costs O(n log n + q + w).** `visit_async_tasks` builds
   the ready, parked, and stopped sets and a map from each waiting
   invocation to its awaited task before the task loop. n is the
   registered tasks, q the queued jobs, w the waiter entries. The
   task-id order and the state precedence of §169 do not change.
4. Program output, trap behaviour, diagnostics, LIR text, and every
   golden do not change.

### 177.2 Acceptance

1. Cost, release, best of three, each binary alone, on the inputs of
   the tracking note:
   - interpreter preparation of the fixed 265-entry set: at most the
     `9647c72a` time (0.182 s);
   - the interpreter at n = 100,000 live tasks: below 1 s, and the
     growth from n = 10,000 to n = 100,000 below 15;
   - one visit at n = 100,000 waiting tasks: below 0.05 s, and the
     growth from n = 10,000 to n = 100,000 below 15;
   - the interpreter corpus and each benchmark workload: within 1.05
     of the pin.
2. Unit tests:
   - a test that lowers a module with many calls and reads the number
     of row copies (one per module), against a control that lowers a
     second module; a copy for each call instruction makes it fail;
   - a registry test that registers and retires tasks in a loop and
     reads the registry length against the bound of rule 2, with a
     same-shape control that holds every task live;
   - a visitor test that reads each state of §169 (READY, PARKED,
     WAITING with its awaited id, ACTIVE, COMPLETE, STOPPED) in one
     visit.
   State the cost of each new gate test (core principle 15). The
   n = 100,000 runs are tracking-note measurements, not gate tests.
3. Trap entry `t107`: five unawaited failed tasks, each held only by a
   reference object, released by one `Context.collect()`. The trap
   reports the first task by id in each tier. A non-test interpreter
   build that releases the handles in registry or allocation order
   reports another task (the round 1 tree reports `fifth`). `t98` has
   two tasks and does not show the order.
4. No other golden moves.

### 177.3 Sections this one amends

- §172.3 item 1: closed. The cause is rule 1's site, not the class
  and Map descriptions or a verifier pass.
- §169.3 items 5 and 6: closed. Item 6 is closed by rule 2.
- §160.3 item 1: restated with the 2026-10-07 measurement; stays open.
