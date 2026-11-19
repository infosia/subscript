<!-- §338 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 338. A check can fail and a test can fire

*(Added 2026-11-19.)* Origin: rank 18 of `specs/tracking/backlog.md`
(§168.3 item 1, §169.3 items 3, 4, 10, §172.3 item 2, §174.3 item 3,
§176.4 item 3, §268.4 item 2, §270.4 item 1, §323.4 item 3). On
2026-11-19 the owner told Claude to start ranks 18 and 19.

Each item is a check that a defect in its subject cannot fail, a
control that is not in the shape of its subject, or a form with no test
(core principle 9). No program changes its output, unless a test of
this section finds a defect. Then the round stops and reports it.

### 338.1 Rules

1. **§168.3 item 1.** The aggregate trap test of
   `runtime/tests/async_budget.rs` also reads, after the clearance, that
   the aggregate frame is freed (a live frame or allocation count of the
   Context). A clearance that keeps the frame fails the test.
2. **§169.3 item 3.** A test calls a host function from generated
   code, and the host function reads the task records: the running task
   is `ACTIVE`. The development tier and the ship tier give the same
   records. The interpreter runs no foreign call (it reports
   `Unsupported`), so it is not in this part. The three-tier inspection
   test (`codegen/src/interpreter/inspection_tests.rs`) also has a
   program whose `READY` head traps, and each tier gives the same
   records. *(Amended 2026-11-19: round 1 stopped because the
   interpreter rejects the foreign call of the host function.)*
3. **§169.3 item 4.** The ring control of
   `runtime/tests/async_inspection.rs` is in the shape of its subject: a
   ring of waiting frames that are not complete, with one change that
   makes the control differ.
4. **§169.3 item 10.** A test reads `async_tasks` and a trap position
   after a rejected reload. The records and the position are those of
   the generation that runs, not of the rejected one.
5. **§172.3 item 2.** The suspension total check fails on each defect
   that it names, in the development tier and in the ship tier:
   1. the frame-lifetime half derives, from the LIR, each assignment
      point of a value that a suspension needs (a frame-local store, an
      environment store, a block-parameter argument on each incoming
      edge). At each such point it requires a native write of that
      field. A JIT block parameter can have one write at the entry of
      its receiving block in place of one write per edge. A write at
      one point does not cover another point: a union of the writes of
      all paths is not the comparison. The check does not seed the
      suspension with the needed set.
   2. the premature-clear check covers block-parameter values and
      branch-edge arguments. The native operations at one point keep
      their order: at an assignment point, the last operation on the
      field is the write, so a clear after the write at the same point
      is premature. *(Amended 2026-11-19 after the third review: a
      clear after the write at the assignment point passed, because the
      check kept the write and the clear of a point as two sets.)*
   3. the finish-clear check reads each exit of the frame (each return
      and the unwind exit), not the presence of one clear.
   Each part has a test in each tier that builds the violating native
   form and reads the failure through the full check. *(Amended
   2026-11-19 after the Phase Review: a write present on one path kept
   the field written on every path, so a missing write on a back edge
   or at a second assignment passed; the JIT finish clear had one tag
   for all exits.)*
6. **§174.3 item 3.** The interpreter trap test of §174 reads the trap
   kind and position, for the zero store and for the `pop` control.
7. **§176.4 item 3.** The start-state description has a check from a
   separate fact: a test runs, in each tier, a generator that has a
   counted parameter of each kind and is dropped before its first
   `next`, and reads after the drop a fact that each count gives: the
   registered task count for a handle, and the count of each argument
   where the tier exposes it. A start description with a missing count
   and one with an extra count each make the test fail.
8. **§268.4 item 2.** A test sends the write error of §268 rule 2
   through the fork and the outcome protocol, and reads the error in
   the parent.
9. **§270.4 item 1.** The control of §270 acceptance 3 counts the
   calls of each release helper of the fixture, so a second end of the
   completed request fails the control. Rule 2 has a test that fails
   if the helper does not clear the slot.
10. **§323.4 item 3.** An accept entry holds the field-address owner
    form: a method of a `@ValueType` field of a temporary object, and
    the method collects. It is Red at the contract pin of §323 (`0e37b789`; it
    printed `9` there). It is not `js-comparable` (C2).

### 338.2 Acceptance

1. Each rule has a test that fails on the defect that it names. The
   coding agent shows the failure once by a change of the subject (not
   of the test), and reverts it. The report gives each failure output.
2. Rule 10: the accept entry fails at the §323 contract pin and passes
   after. The LIR text golden does not change unless the entry is async
   or a generator.
3. No other corpus entry and no other golden changes. The full gate
   passes.
4. Each test that this section adds or changes states its measured
   cost (core principle 15).

### 338.3 Sections this one amends

- §168.3 item 1, §169.3 items 3, 4, 10, §172.3 item 2, §174.3 item 3,
  §176.4 item 3, §268.4 item 2, §270.4 item 1, and §323.4 item 3
  close.

### 338.4 Phase Review

Form. Each rule has a test that fails on its defect; the coding agent
showed each failure once by a change of the subject, then reverted it.

| Rule | Test | Subject change that fails it |
|---|---|---|
| 1 | `aggregate_trap_stays_pending_until_clearance` reads the aggregate frame after clearance | clearance keeps the aggregate frame |
| 2 | `generated_host_call_reads_the_active_task_in_both_native_tiers`; a `READY` head that traps in `async_task_snapshots_match_three_tiers` | `ACTIVE` reported as 2; a trapped `READY` frame reported as `STOPPED` |
| 3 | `wait_ring_and_parked_control`: three waiting frames, the control parks the last | `awaited_task_id` off by one |
| 4 | `rejected_reload_preserves_task_records_and_trap_positions` | a rejected reload resets the position table |
| 5 | the suspension total check (`codegen/src/lower/func/frame_root_tests.rs`) | see below |
| 6 | the §174 trap test reads kind `UncaughtException` at 10:40 | the interpreter reports `Internal` |
| 7 | `dropping_unstarted_generators_releases_each_counted_parameter_kind` | a start description with a missing count, and with an extra count |
| 8 | `retained_write_error_reaches_the_parent` (`ulimit -f` as the failing writer) | the entry ignores the write error |
| 9 | `device_release_does_not_end_a_completed_request_again`, `subscription_helper_clears_its_slot_before_device_release` | a helper does not clear its slot |
| 10 | a487: a method of a `@ValueType` field of a temporary collects | at `0e37b789` the development tier and the ship tier print `9`; now `1` |

Rule 5. The check derives from the LIR each assignment point of a value
that a suspension needs, and requires a native write there, in each
tier. The native operations at one point keep their order. Each exit
(each return and the unwind exit) has its own test tag in the JIT and
its own region in C. The source tags are `cfg(test)` only; no
generated code changes. The check reads 678 coroutines and 1,160
suspension points. Its cost, debug, best of three: 13.88 s at the pin,
8.38 s after. Each part has a negative test in each tier through the
full check.

Four reviews ran. The first found that the JIT moved each write of a
block-parameter field to the block entry, so a missing edge write
passed. The second found that a union of the writes of all paths kept a
field written after one write, so a missing back-edge write or a
missing write at a second assignment passed, and that the JIT finish
clear had one tag for all exits. The third found that a clear after the
write at the same point passed. Rule 5 changed after the second and
the third. The fourth found no CRITICAL or MAJOR finding. Its realistic
MINOR finding is fixed: two C negative tests had no passing control.

### 338.5 Open

1. Contrived: a write on one arm of a branch inside one point covers
   the other arm (the JIT absent-branch zero of a function value, and
   the C `tN_then`/`tN_else` arms of one edge). Two edges from one
   block to one target share one point.
2. Contrived: the C reader takes only `memset` and `= NULL` as a clear;
   the emitter zero forms `0` and `(SubFn){0}` read as writes. A partial
   clear of a multi-word field is not seen.
3. The kinds that need environment storage (`MakeClosure` with
   captures, `Copy`, `Cast`) are a copy of the emitter call sites. A new
   kind that both emitters forget passes.
4. A store through `AddressOfLocal` into a frame local is not an
   assignment point. The golden gate sees a wrong address.
5. Contrived: the C `coroutine_done:` region is exempt from the finish
   check; the test tags alias above about 65,000 blocks.
