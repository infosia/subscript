<!-- §356 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 356. The mark of a collection runs in steps

*(Added 2026-11-24.)* Origin: the owner decision of 2026-11-24, after
the measurements G14 and G15 of
`specs/tracking/automatic-collection.md`: a mark in steps lands
first, then a sweep outside the pause, then a minor collection as an
optional mode. This section is the first of the three.

Problem, measured at `b1d35082` in the ship tier (G14, G15):

1. **One collection is one pause, and the pause follows the live
   graph.** A graph of 1,400,000 small objects with links between
   them (64 MiB) gives a pause of 177 to 180 ms. A graph of 350,000
   objects (16 MiB) gives 28 to 33 ms. A game form with 200,000
   objects that stay gives 36 to 46 ms.
2. **The host has no control that makes that pause shorter.** The
   headroom changes the count of the collections and the sweep. It
   does not change the mark (G10). The host-timed level moves the
   pause; it does not divide it. Invariant 2 states that the host
   controls each pause.
3. **A prototype of a mark in steps gives a largest pause of 6.3 to
   10.2 ms at 64 MiB**, 2.2 to 3.7 ms at 16 MiB, and 2.5 to 5.7 ms in
   the game form. The mark reads 1 to 7 % more words. The peak
   reserved memory is 0.5 to 7.6 % higher.

### 356.1 Rules

**The cycle**

1. **A collection is a cycle: a start, mark steps, and an end.** The
   start takes the roots into a work list. A mark step takes words
   from the work list and marks the blocks that they name. The end
   reads the roots again, empties the work list, does the releases of
   §348 and of the counted fields, and sweeps. Each of the three is a
   pause. Script code runs between them.
2. **A step has a budget in words.** The unit is the word of §353
   rule 5. A step stops when it has read the budget. A step reads at
   most the budget plus the words of one small block.
3. **The scan of a large payload is divided.** A step that reaches
   its budget inside a payload keeps the position, and a later step
   continues there. A payload of 1,000,000 references does not make
   one step longer than another.
4. **A block that is allocated during a mark is live for that
   cycle.** The allocation gives it the mark.
5. **The end is one pause, and its mark has the budget.** The end
   reads the roots and marks with the budget. If the work list is
   not empty after the budget, the pause ends as a step, and the
   next pause tries the end again. If it is empty, the flag of rule
   12 clears, and the releases and the sweep follow in the same
   pause. Each try reads the root words again, and they count in
   the words of the pause. So the mark part of an end is not above
   rule 2 plus the root words, for each program: a root can hold the only reference to a large graph
   that has no mark, when the program moved it from a field to a
   local or to a module variable during the cycle. After the end no
   reachable block is without a mark at the sweep (rule 1 of §342).
   The sweep does not change in this section. A collection of rule
   8 ends a cycle with no budget.
6. **Where a step runs.** In mode 1 a step runs where a collection
   runs today: at a poll and at the outermost exit, through the
   pending flag. During a cycle the allocation sets the flag after
   each part of the headroom, so that the mark work follows the
   allocation. A part is not smaller than 4 KiB. The allocation path
   gets no added work when no cycle is in progress, and an allocation
   never runs a step (§342 rule 5).
6a. **A step that leaves the work list empty makes the next pause
   the end.** The end empties the work list in each case (rule 1).
   Without this rule a program that stores one reference between
   two pauses never reaches the end.
6b. **The mark work follows the stores also.** During a cycle the
   barrier counts the references that it receives, in the work
   list or not. After each budget of them it sets the pending flag,
   so that the next poll or the next outermost exit runs one pause.
   A program that changes references and does not allocate ends its
   cycle. A count of the references that go into the work list does
   not do this: after each changed block is in the list, nothing
   sets the flag (measured: the cycle did not end).
7. **A cycle starts before the limit of §346**, so that the mark can
   end near the limit. If the bytes requested since the start of the
   cycle exceed one limit, the cycle is late: from there each poll
   and each outermost exit runs one step, until the end. A step of a
   late cycle keeps the budget. The record of rule 17 counts each
   cycle that became late. Measured with a late step that had no
   budget: one pause of the game form read 910,720 words (6.6 to
   8.7 ms) against a budget of 50,000.
8. **A collection that must be exact runs in one pause.** These are
   the explicit collection (`Context.collect()`,
   `subscript_rt_ctx_collect`), the detection collection (§342 rule
   14), and a collection of mode 2. If a cycle is in progress, they
   end it first, and then they run one collection with no step. So
   §342 rules 13, 15, and 20 do not change.
9. **`Context.free` during a mark is valid.** The mark does not read
   a block that the program released, and a block that takes its
   place is a block of rule 4.

**The barrier**

10. **A store of a reference into a block during a mark gives the
    reference to the work list.** A block is in the work list at
    most one time: the barrier gives a block that has no mark a
    state "in the work list", and it gives nothing for a block that
    has that state or the mark. So the work list is not larger than
    the count of the blocks, for each count of stores. Measured
    without this: 1,000 stores of one reference in mode 0 added
    1,000 words. The work is after the store. It
    applies to each store of a word that can hold a reference, into
    memory that the mark reads as a payload: a field, an element, a
    frame of a coroutine, a captured variable, the storage of a Map
    or a Set, a result array, a completion value.
11. **A root has no barrier.** The stack, a root slot, a module
    variable, and an operation root (§345) are read again at the end.
12. **The Context holds a flag byte "a mark is in progress"** in its
    fixed prefix, beside the pending flag. Generated code reads the
    flag after a store of rule 10: one load and one branch, with the
    call on the cold path. Each Context has its own flag.
13. **The LIR states which stores are stores of rule 10.** Each
    instruction kind states, by a total match, if it writes a word
    into a block. The static type of the stored value states if the
    word can hold a reference. The C emission derives the barrier
    from the two facts. A store of a number has no barrier.
14. **The runtime does the work of rule 10 for its own writes.**
    After the runtime writes references into a block during a mark,
    each reference goes to the work list, or the block does. This
    includes a block that the runtime fills by a copy (a growth, a
    result of `map`, `filter`, `slice`, `concat`, a spread).
14a. **A move of references inside one block does not start its
    scan again** (`sort`, `reverse`, `splice`, `shift`, `unshift`,
    `copyWithin`, a rehash). A move adds no reference to the block.
    If the scan of the block is complete, the runtime does nothing.
    If the scan is divided, the position stays, and the words of the
    part that the scan read go to the work list: a move can bring a
    reference there from the part that the scan did not read. The
    cost has the order of the move. Measured with a form that put
    the block in the work list again: a loop of `shift` on an array
    larger than one budget started the scan again at each step, and
    the cycle did not end. An operation that also writes a value
    (`unshift`, `fill`) gives that value (rule 14). The write-back
    of `sort` is a write of rule 14, not a move: its source is a
    buffer of the operation, which is a root.
15. **The host needs no barrier.** A foreign call can leave the
    former value or null in a pointer that the mark reads (§325 rule
    4), and no host function stores a reference outside a call.

**Host control**

16. **The host sets the budget.**
    `subscript_rt_ctx_set_collection_step(ctx, budget_words)`. A
    budget of 0 makes each collection one pause, as before this
    section. The default is 50,000 words: in G15 a step of that
    budget took 0.3 to 0.85 ms. The call returns 0, or -1 at a script
    depth that is not zero. If a cycle is in progress, the new budget
    applies to its next step; a budget of 0 ends the cycle at the
    next poll. In mode 0 no poll serves a cycle: it stays until the
    host ends it with `subscript_rt_ctx_collect_step` or with
    `subscript_rt_ctx_collect`. A worker Context takes the budget of
    its parent at the
    spawn (§342 rule 18).
17. **The record of §353 gets three fields at its end**:
    `mark_steps`, `forced_ends`, and `max_pause_words_read`. For a
    cycle in steps, `last_ns` and `max_ns` are of one pause (the
    start, one step, or the end), and `total_ns` is the sum of the
    pauses. `automatic_collections` counts the cycles that ended.
    `last_words_read` and `max_words_read` are of one whole cycle.
    `max_pause_words_read` is the largest count of words that one
    pause read. `forced_ends` counts the cycles that became late
    (rule 7). In mode 1 no step reads more than rule 2 allows.
18. **A host-timed step.** `subscript_rt_ctx_collect_step(ctx,
    budget_words)` is valid at script depth zero, in mode 0 and in
    mode 1. If no cycle is in progress, it starts one. Then each
    call runs one step, and the call after a step that left the work
    list empty runs the end (rule 6a). It returns 1 if a cycle is in progress after the
    call, 0 if the call ended the cycle, and -1 for a script depth
    that is not zero or a budget of 0. A host of the host-timed level
    calls it at each frame until it returns 0.
19. **Mode 3 is the step stress mode, for tests.** Each poll and each
    outermost exit runs one step with the budget that the host set. A
    new cycle starts when one ends. Each end runs the check of rule
    20 and stops the process with a message if the check fails. A
    cycle of mode 3 has at most 8 steps: the next pause is the end.
    Without that
    limit one cycle can last for the whole program, because each
    store during a mark adds to the work list. Each block is then a
    block of rule 4, and no store can fail a check. Measured with a
    budget of 4 words: 274 of 610 entries ended no cycle.
20. **The check of the mark.** At an end, after the mark and before
    the sweep, a separate walk from the roots finds each reachable
    block with a second record, not with the marks. The check fails
    if a block of that record has no mark. The message names the
    count, and for the first blocks the class of the block, the
    class of a block that holds it, and the offset of the word. The
    walk reads a payload as the mark does (the scan is conservative,
    §342.5 item 1), so a number that equals the address of a block
    can fail the check.
20a. **The check of each pause.** The check of rule 20 can fail only
    if the program also drops each other path to the block before
    the end. So mode 3 has a second check, after each step and
    before each end: no block that has its mark and whose scan is
    complete holds a word that names a live block with no mark,
    unless that block is in the work list. A block of rule 4 is such
    a block. The check does not depend on what the program does
    after the store. It stops the process with the message of rule
    20.

**Tiers**

20b. **A Context with a mark trace has no step form.** The trace of
    `SUBSCRIPT_MARK_TRACE` records the path to each block in one
    whole mark. The Context reads the variable for this rule one
    time, at its creation.
21. **This section changes the ship tier.** The dev tier and the
    interpreter run each collection in one pause, and their code has
    no barrier. The flag of rule 12 is never set there. The functions
    of rules 16 and 18 are valid in each tier: where the Context has
    no step form, the budget has no effect, and
    `subscript_rt_ctx_collect_step` runs one whole collection and
    returns 0. §342 rule 2 makes the difference not observable from
    script code.

### 356.2 Acceptance

Each item names the result that fails the section.

1. **Red at the pin.** A runtime test builds a ship Context with an
   array of 1,000,000 references to small objects, sets a budget of
   50,000 words and a small headroom, and allocates until one cycle
   ends. `max_pause_words_read` is under 60,000. At the pin the
   record has no such field; the same heap gives `last_words_read`
   above 1,000,000 in one pause.
2. **The check fails when a barrier is absent.** A runtime test
   starts a cycle, runs steps until a block A has its mark and a
   block B has none, writes the address of B into A with a raw store,
   removes each other reference to B, and runs the check. The check
   reports 1 block. The same test with the barrier call after the
   store reports 0.
3. **Runtime tests, each with the check at each end**: an allocation
   during a mark stays live; `Context.free` during a mark, and a
   block that takes the place; a growth of an array and of a Map
   during a mark; `sort`, `reverse`, `splice`, `fill` on an array
   whose scan is in progress; a completion value and a result array
   of `Promise.all`; a forced end, with its count; an explicit
   collection during a cycle gives the live bytes that a Context
   with a budget of 0 gives; a budget of 0 gives the collections of
   the pin.
4. **The step stress sweep.** The test of §342.2 item 1 gets a run of
   each ship entry in mode 3, with a small budget, on the executable
   that the mode 2 run built: no second compile and no second link.
   Fail: a difference from the golden, or a message of rule 20 or
   rule 20a. The test prints the count of the cycles that ended. The
   test states its cost. A control is measured one time and recorded:
   with no barrier in the emitted C, the count of the entries that
   rule 20a stops, against the count that rule 20 stops.
5. **Rule 13 is total.** A match with no wildcard arm gives the fact
   of each instruction kind.
6. **The record.** The header declares the three fields and the two
   functions. The test of the header (§353.2 item 6) reads the size
   and each offset.
7. **The budget of 0.** With `subscript_rt_ctx_set_collection_step(
   ctx, 0)` the stress sweep of mode 2 and the tests of §353 give
   the results of the pin.
8. **No golden moves.** The mode 2 sweep passes in the three tiers.
9. **Cost.** `perf-gate --gate` meets each criterion. The figure of
   `collect` in the ship tier is recorded (G15: 4.4 to 6.5 % over
   the pin with the barrier in the code).
10. **The pauses, measured one time and recorded** (not a gate):
    workload 1 of G14 at 16 MiB and at 64 MiB, and the game form at
    200,000 objects, with the default budget: the largest pause, the
    median step, the run time, and the peak reserved memory, against
    a budget of 0.
11. **Documents.** The header, the C tutorial, and the Rust tutorial
    state the budget, the host-timed step, and the three fields.
12. `tools/gate.sh full` passes.

### 356.3 Sections this one amends

- §342 rule 7 and rule 8: a poll and an outermost exit serve the
  pending flag with one pause of a cycle (the start, a step, or the
  end), not with one whole collection.
- §342 rule 17: mode 3 exists.
- §353 rule 2 and rule 4: the three fields, and the meaning of the
  times for a cycle in steps (rule 17 of this section).
- §353.6 item 1 stays open: the record has no size field.

### 356.4 Rounds

| Round | Content |
|---|---|
| 1 | Runtime: the cycle (rules 1 to 9), the flag, the entry of the barrier, rules 16 to 20, the record; acceptance 1, 2, and the parts of 3 that need no runtime writer. The default budget stays 0 in this round. |
| 2 | Runtime: rule 14 for each writer of the runtime; the rest of acceptance 3. |
| 3 | Rule 13, the C emission, the header; acceptance 4 and 5. |
| 4 | The default budget of 50,000; the documents; acceptance 6 to 12. |

### 356.4a Phase Review

Form.

- `runtime/src/context/mark_cycle.rs` holds the cycle: the work
  list, the divided scan, the start, the step, the try of the end,
  the late state, the entries of the barrier, and the checks of
  rules 20 and 20a. The flag byte is at offset 29 of the Context
  prefix. A header has a state "in the work list" beside the live
  state and the mark.
- The allocation path has no added branch: the test of §346 reads a
  value that moves in parts during a cycle.
- `codegen/src/lir/block_store.rs` is the fact of rule 13, as an
  analysis. The LIR text does not carry it. `codegen/src/cemit/
  barrier.rs` emits the test of the flag in the form of the poll.
- The step sweep is a second run of each ship executable of the
  mode 2 sweep, at budgets of 64, 16, and 4 words.

Measured.

| Item | Result |
|---|---|
| Step sweep, mode 3, 610 ship entries | each equal at each budget; 10,426, 7,448, and 3,967 cycles ended; no message of rule 20 or 20a |
| Mode 2 sweep | dev JIT 612 of 612, ship 610 of 610, interpreter 500 of 500 |
| Cost of the sweep | 46.9 s in a release build, against 45.4 s with no mode 3 run |
| Control: no barrier in the emitted C and in the runtime | rule 20a stops 22, 25, and 23 entries at the three budgets, 40 together; each uses async functions or a Promise. Rule 20 alone, before the step limit of mode 3: 7 entries |
| Control: one point off | each emission point and each runtime writer that §356.5 items 6 to 8 do not name stops a corpus entry, a program of `mark_barrier_points.rs`, or a runtime test |
| Acceptance 1 | budget 50,000: `max_pause_words_read` under 60,000; budget 0: one pause above 1,000,000 words |
| Workload 1, 16 MiB, largest pause | 1.57 to 1.59 ms, against 29.4 to 32.4 ms with a budget of 0 |
| Workload 1, 64 MiB, largest pause | 6.73 to 6.89 ms, against 181.5 to 183.4 ms |
| The game form, 200,000 objects, largest pause | 2.78 to 2.97 ms, against 45.3 to 46.3 ms |
| The largest count of words of one pause, the three workloads | 50,004 |
| Run time against a budget of 0 | 0.95 to 0.98 s against 0.93 to 0.94 s; 5.42 to 5.45 s against 5.18 to 5.20 s; 3.14 to 3.36 s against 3.05 to 3.06 s |
| `perf-gate --gate` | each criterion met; `a22` ship 1.33x, `collect` ship 0.97x (30.6 to 30.9 ms; 29.9 to 30.6 ms before the section) |
| Goldens | none moved |

The figures of the workloads are in
`specs/tracking/automatic-collection.md`, G16.

Defects that the rounds found and closed, each with a test that
failed first.

| Defect | Close |
|---|---|
| The end check alone stops 7 entries with no barrier | rule 20a |
| One cycle of mode 3 lasted for the whole program, so no store was tested | rule 19: 8 steps |
| A cursor of `for…of` saved at a suspension had no barrier (`a353`, found by the sweep) | the range barrier; a total match on the value kind |
| A program that stores one reference between two pauses did not reach the end | rule 6a |
| The work list grew with each store | rule 10 |
| A cycle with stores and no allocation did not end | rule 6b |
| A step with no budget read 910,720 words | rule 7: a late cycle keeps the budget |

Phase Review (a subagent with no context, on the staged diff): no
CRITICAL, 1 MAJOR, 6 MINOR. A second review of the fix: no CRITICAL,
no MAJOR, 4 MINOR.

| Finding | Close |
|---|---|
| MAJOR, realistic: a move inside a large array started its scan again, so a loop of `shift` kept the cycle open | rule 14a; three tests that do not end with the former form |
| Related: the end read a large payload or a graph at a root in one pause | rule 5: the mark of the end has the budget; two tests that fail with no budget (4,002 and 8,193 words) |
| MINOR, contrived: a barrier between the mark of the end and the sweep | the flag clears before the releases (rule 5) |
| MINOR, realistic: the mark trace saw the end only | rule 20b |
| MINOR: two runtime entries had no direct test | one test of the range entry and of the written-words helper |
| MINOR: the documents did not state the failed-task check during a cycle | the header and the C tutorial |
| MINOR: two control figures with no conditions; a comment that named former behaviour | corrected |
| MINOR, contrived (second review): the write-back of `sort` was a move | rule 14a; a test that fails with the former call |
| MINOR, contrived (second review): a compaction of a Map before a failed growth gave nothing | the call is at the compaction |
| MINOR (second review): a cycle that became late in the tries of the end had no count | counted; a test |
| MINOR, realistic (second review): an operation root of the size of the data | §356.5 item 5 |
| MINOR, contrived (second review): an end after an internal error in a debug build | §356.5 item 9 |

`tools/hygiene.sh`: exit 0.

`tools/gate.sh full` on the tree before this record: exit 0, debug
4223 passed, release 4204 passed, 0 failed, 10 ignored in each. No
golden moved.

### 356.5 Open

1. The end is the sweep: 6.4 ms at 64 MiB (G15). A sweep outside the
   pause is the next section.
2. The dev tier and the interpreter have no step form (rule 21).
3. A minor collection as an optional mode (G14) uses the stores of
   rule 13.
4. The marks of §348 inside the end (the tasks with holders outside,
   the failures of ending tasks) have no budget. A large graph that
   only a completed task with an outside holder reaches is marked in
   one pause.
5. An operation root can have the size of the data: a `sort` with a
   comparator holds two buffers of n words. The start and each try of
   the end read them (rule 5). A sort of 1,000,000 references during
   the tries of an end gives pauses of about 2,000,000 words.
6. Three runtime writers have their barrier and no test that fails
   with that barrier alone off: the completion of a host operation,
   the counted array operation, and `receive`. The barriers of
   `unshift`, of the result array of `Promise.all`, and of the child
   of an `AsyncCall` are covered by another barrier (measured).
7. No synchronous corpus entry fails the step sweep with each barrier
   off: the corpus programs are small. The programs of
   `codegen/tests/mark_barrier_points.rs` carry the synchronous
   stores. The frame environment copy has no program that fails with
   that point alone off.
8. The facts of rule 13 for `BoxBoundaryValue` and `AddressOfValue`
   reach no barrier today: a value class cannot hold a reference
   (S100).
9. In a debug build, an end that finds a defect of the array tails
   (an internal error) is counted and does not end the cycle.
10. `ReloadSession` has no function that sets the budget: it has none
    that sets the mode.
