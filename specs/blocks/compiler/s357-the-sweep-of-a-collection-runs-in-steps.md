<!-- §357 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 357. The sweep of a collection runs in steps

*(Added 2026-11-24.)* Origin: the owner decision of 2026-11-24 (§356
is the first of three sections; this is the second), and the
measurement G17 of `specs/tracking/automatic-collection.md`.

Problem, measured at `6c889b41` in the ship tier (G16, G17):

1. **After §356 the largest pause is the sweep of the end.** It is
   6.7 to 7.7 ms for a live graph of 64 MiB, 2.8 ms for the game form
   with 200,000 objects, and 0.72 to 0.75 ms for a program with
   1.5 MiB live and 16 MiB of headroom. The budget of §356 does not
   reduce it: the sweep reads each slot below the bump marks (G10).
2. **A prototype of a sweep in steps gives an end of 0.003 to
   0.05 ms after its mark.** The largest pause becomes 1.2 to 1.6 ms,
   0.86 ms, and 0.40 ms in the three programs, and it is a mark
   step. The allocation path and the run time do not change.
3. **A form that hides the free slots until the sweep reaches them
   grows the reserved memory** (545 MiB against 286 MiB). A form that
   exchanges the meaning of the two header state words keeps the
   free lists, and the memory stays within 7 %.

### 357.1 Rules

**The two state words**

1. **A Context of the ship arena holds which header state word is
   "live" and which is "marked".** A mark writes the marked word. At
   the end of a cycle, after the mark and the releases that read the
   marks, the Context exchanges the two. A block that the mark
   reached is then live with no write. A block with the former live
   word is a block to release. A new block gets the live word of the
   Context.
2. **Each reader of a header state reads the words of the Context.**
   No code of the ship arena compares a header with a constant live
   word or a constant marked word. The dev tier and the interpreter
   keep constant words: their Context never exchanges (rule 14).
3. **A block to release is not reachable.** No reference to it is
   read by the program, apart from a use after `Context.free` or a
   stale handle (`collisions.md` Q6). The walks that count live
   blocks count it as live until the sweep releases it.

**The sweep**

4. **The end of a cycle does not sweep the small blocks.** After the
   mark of the end (§356 rule 5): the releases of §348 and of the
   counted fields, the regex values, the large blocks, and the
   exchange of rule 1. Then a sweep is in progress.
5. **A sweep step releases the blocks to release in a part of the
   arena.** It reads the slots from a position that the Context
   keeps, and it puts each block to release on its free list. It
   writes nothing for a live block. A step reads at most 5 slots for
   each word of the budget of §356 rule 16: with the default budget,
   250,000 slots (G17: 0.25 to 0.49 ms for 262,144 slots).
6. **Where a sweep step runs.** In mode 1, at a poll and at the
   outermost exit, through the pending flag, as a mark step (§356
   rule 6). The allocation sets the flag after each part of the
   bytes that are left before the next cycle start, so that the
   sweep ends after 1/8 of them. A part is not smaller than 4 KiB.
   A block to release that the sweep did not reach is not a free
   slot, so a sweep that ends late costs memory. Measured: the
   reserved memory is 4 to 15 % above §356 with 1/2, and 1 to 6 %
   with 1/8; the run time is not longer with 1/8.
   The allocation path gets no added work, and an allocation never
   runs a sweep step (§342 rule 5).
7. **An allocation during a sweep is valid in each position.** It
   takes a free slot or a bump slot as before, and the block is live
   by rule 1. A sweep step does not release it.
8. **`Context.free` during a sweep is valid**, for a live block and
   for a block to release (a stale handle of Q6 aside, the program
   holds no reference to the second kind).
9. **No cycle starts while a sweep is in progress.** If the
   requested bytes reach the start of the next cycle first, each
   poll and each outermost exit runs one sweep step until the sweep
   ends, and the cycle starts after it. No sweep step reads more
   than rule 5 allows. The record counts each such sweep.
10. **A collection that must be exact completes a sweep first**, in
    the same pause and with no budget (§356 rule 8). A budget of 0
    gives a sweep in the pause of the end, with the exchange of rule
    1.
11. **The live bytes while a sweep is in progress** include the
    blocks to release that the sweep did not reach. The value of the
    limit of §346 after a collection is the value that a sweep of
    one pause gives: the Context sets it when the sweep ends, and
    until then the limit of the last sweep that ended applies. A
    limit from the live bytes at the end of the cycle includes the
    blocks to release, and it grows in each cycle (measured:
    388 MiB reserved for a live graph of 16 MiB). §342
    rule 20 does not change: the metrics are equal in the tiers
    after an explicit collection.

**Host control and the record**

12. **`subscript_rt_ctx_collect_step`** (§356 rule 18) returns 1
    while a sweep is in progress, and each call runs one sweep step.
    It returns 0 when the call completed the sweep. No new function
    and no new budget exist: the budget of §356 rule 16 gives the
    size of a sweep step by rule 5.
13. **The record gets three fields at its end**: `sweep_steps`,
    `max_pause_slots_swept`, and `late_sweeps` (rule 9). A sweep
    step is a pause in `last_ns`, `max_ns`, and `total_ns`.
    `last_live_bytes_after` is of the last sweep that ended.
    `automatic_collections` counts at the end of a cycle, as in
    §356.

**Tiers and tests**

14. **This section changes the ship tier.** The dev tier and the
    interpreter sweep in the pause of their collection.
15. **Mode 3 runs the sweep in steps**, one step at each poll and
    each outermost exit. A sweep of mode 3 has at most 8 steps. At
    the completion of a sweep it runs a check and stops the process
    with a message if the check fails: a walk from the roots finds
    no released block and no block to release; a walk of each slot
    finds no block to release.

### 357.2 Acceptance

1. **Red at the pin.** A runtime test builds a ship Context with
   1,000,000 small blocks that are not reachable and a budget of
   50,000 words, and runs a cycle to the completion of its sweep.
   `max_pause_slots_swept` is not above 250,000 plus the slots of
   one chunk, and `sweep_steps` is above 3. At the pin the record has
   no such field, and the end releases each block in one pause.
2. **The same blocks are released.** For a heap that the test
   builds, the count of the live blocks and the live bytes after a
   sweep in steps equal those after a collection with a budget of 0.
3. **Runtime tests, each with the check of rule 15 at the completion
   of the sweep**: an allocation during a sweep stays live, in a
   free slot of a part that the sweep did not reach and in a bump
   slot; `Context.free` during a sweep; an explicit collection
   during a sweep gives the live bytes of a Context with a budget of
   0; a sweep that the next cycle start reaches (rule 9: `late_sweeps`
   is 1, `max_pause_slots_swept` keeps its bound, the cycle starts
   after it); a Context and a worker Context dropped during a sweep;
   a large block that is not reachable is released at the end; a
   budget of 0 set during a sweep completes it at the next poll.
4. **Rule 2 is total.** A search of the runtime finds no compare of
   a header with a constant state word in code that the ship arena
   runs. The test of the dev tier lifetime check passes.
5. **The check fails when a block is released wrongly.** A runtime
   test gives a reachable block the word of a block to release
   before a sweep step, and the check of rule 15 reports it.
6. **The step stress sweep** (§356.2 item 4) runs with the sweep in
   steps and the check of rule 15. Fail: a difference from the
   golden, or a message. The test states its cost.
7. **The record and the header.** The header declares the three
   fields; the test of the header reads the size and each offset.
8. **No golden moves.** The mode 2 sweep passes in the three tiers.
9. **Cost.** `perf-gate --gate` meets each criterion.
10. **The pauses, measured one time and recorded** (not a gate): the
    workloads of G16 and the island form of G17, with the default
    budget: the largest pause, the largest sweep step, the sweep
    steps, the run time, the reserved memory at the end, against
    §356.
11. **Documents.** The header, the C tutorial, and the Rust tutorial
    state that the sweep runs in steps, the three fields, and the
    meaning of the live bytes during a sweep.
12. `tools/gate.sh full` passes.

### 357.3 Sections this one amends

- §356 rule 5: the sweep does not follow the mark of the end in the
  same pause, apart from a collection of rule 10.
- §356 rule 18: the return value while a sweep is in progress.
- §353 rule 2: the three fields.

### 357.4 Rounds

| Round | Content |
|---|---|
| 1 | Runtime: rules 1 to 13, the readers of rule 2, the record, the header; acceptance 1 to 5 and 7. |
| 2 | Rule 15 in mode 3 and the stress sweep; the documents; acceptance 6 and 8 to 12. |

### 357.4a Phase Review

Form.

- `runtime/src/context/sweep_steps.rs` holds the exchange, the
  position and the limits of a sweep, the sweep step, the parts of
  rule 6, the late sweep, the completion, and the check of rule 15.
  The two words are fields of the cycle state of §356. The grey
  state and the dead state keep constant words.
- The end releases the large blocks and the regex values with no
  mark, then it exchanges. The limits are the bump mark of each
  chunk at the exchange: a count of words in the order of the
  chunks.
- The allocation path does not change: it writes the word that
  §356 rule 4 gave it, and it tests the value of §346.
- A test reads each source file of the runtime and finds a constant
  state word only at the definitions and at the first value of the
  two words.

Measured.

| Item | Result |
|---|---|
| Acceptance 1 | budget 50,000: 4 sweep steps, at most 250,000 slots plus one chunk in a pause; budget 0: one pause of 1,000,000 slots |
| Step sweep, mode 3, 610 ship entries | each equal at budgets 64, 16, and 4; 7,993, 6,093, and 3,456 cycles ended; 7,763, 6,009, and 4,733 sweep steps; no message |
| Mode 2 sweep | dev JIT 612 of 612, ship 610 of 610, interpreter 500 of 500 |
| Cost of the sweep test | 46.5 to 48.9 s in a release build |
| Control: a block that is allocated during a sweep gets the word of a block to release | rule 15 stops 263, 253, and 191 runs: 280 entries |
| Control of §356 with this section: no barrier | rule 20a of §356 stops 19, 24, and 25 entries, 39 together (22, 25, 23, and 40 before) |
| Largest pause, default budget | 0.75 to 0.83 ms at 16 MiB, 1.19 to 1.44 ms at 64 MiB, 0.82 to 1.11 ms in the game form, 0.40 to 1.08 ms in the island form |
| Reserved memory at the end against §356 | +2.8 %, +2.3 %, +0.8 %, +5.7 % |
| Run time against §356 | 0.94 to 0.97 s against 0.95 to 0.98 s; 5.40 to 5.48 s against 5.42 to 5.45 s; 3.13 to 3.24 s against 3.14 to 3.36 s; 0.48 to 0.52 s against 0.47 s |
| `perf-gate --gate`, two runs | each criterion met; `a22` ship 1.32x and 1.33x, `collect` ship 0.97x and 0.98x |
| Goldens | none moved |

The figures of the workloads are in
`specs/tracking/automatic-collection.md`, G18.

Defects that the rounds found and closed.

| Defect | Close |
|---|---|
| A limit from the live bytes at the end of the cycle grew in each cycle (388 MiB reserved for 16 MiB live) | rule 11: the limit of the last sweep that ended applies until the sweep ends |
| A sweep that ends at 1/2 of the bytes before the next cycle gave 4 to 15 % more reserved memory | rule 6: 1/8 |

Phase Review (a subagent with no context, on the staged diff): no
CRITICAL, no MAJOR, 3 MINOR.

| Finding | Close |
|---|---|
| MINOR, realistic: an exact collection during a sweep recorded the completion of the sweep as a pause of its own | the sweep completes inside the pause of the collection; the test reads `sweep_steps`, `explicit_collections`, and `max_pause_slots_swept` |
| MINOR, realistic: comments stated that the sweep follows the end in the same pause, and a constant live word | corrected |
| MINOR, contrived: the text for a host gave the size of a sweep step with no word on mode 3 | the header and the C tutorial |

`tools/hygiene.sh`: exit 0.

`tools/gate.sh full` on the tree before this record: exit 0, debug
4238 passed, release 4219 passed, 0 failed, 10 ignored in each. No
golden moved.

### 357.5 Open

1. The dev tier and the interpreter have no sweep in steps (rule 14).
2. The large blocks are released at the end, in one pause.
3. The count and the time of the pauses inside one frame of a host
   are not measured: the workloads run each frame inside one call.
4. A program with 1.5 MiB live and 16 MiB of headroom runs 2 to 10 %
   longer than with a budget of 0: the cycle in steps starts at 7/8
   of the limit (§356 rule 7), so the count of the cycles is 88
   against 77.
