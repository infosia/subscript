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
