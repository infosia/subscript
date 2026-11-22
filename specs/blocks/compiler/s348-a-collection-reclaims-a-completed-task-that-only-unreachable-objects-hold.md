<!-- §348 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 348. A collection reclaims a completed task that only unreachable objects hold

*(Added 2026-11-22.)* Origin: §342.5 item 2, and the measurement G7 of
`specs/tracking/automatic-collection.md`.

Problem, measured at `5bbb167c`:

1. **A typical program keeps memory that no root reaches.** An async
   method returns `this`, and the object stores the handle
   (`this.loading = this.load()`). After the program drops the object,
   each collection keeps the object, its fields, and the frame: 104
   bytes for each iteration in the dev tier, 128 in the ship tier. An
   async function that returns its parameter, with the handle stored
   in an array of that parameter, keeps 176 and 224 bytes. The same
   shapes with an `i32` result keep nothing.
2. **The cause is two root sets.** The collector takes each
   registered frame as a root, and the completion value of each frame
   as a root (`runtime/src/context/memory.rs`, the sets `async_frames`
   and `completion`). The completion value reaches the object, the
   object holds the one count of the frame, and the release of
   unreachable counted fields (`runtime/src/context/counted.rs`) never
   sees the object.
3. **The rule that these sets follow is older than the collector of
   §342.** §70.3 rule 6 states that a cycle leaks. That was the cost of
   a count with no collector behind it. A program at the automatic
   level has no statement that ends the cycle.

### 348.1 Rules

Terms. A **completed task** is a registered frame of an async function
call that has a completion, has no waiter, and is in no scheduler
state (ready, parked, stopped, blocked, active). A **heap holder** of
a frame is a counted field of an object, an element of a counted
array, or a value of a counted Map that holds one count of the frame.
An **outside holder** is each other owner of a count: a local of a
script function, the host, a `TaskGroup`, an aggregate.

1. **A completed task is not a root by itself.** The collector does
   not take a completed task, its completion value, or its completion
   Error object as a root because the frame is registered.
2. **A completed task that the mark reaches keeps its completion.**
   If the mark reaches a completed task, the mark reads the
   completion value and the completion Error object of that task.
3. **A completed task with an outside holder is a root.** After the
   mark, for each completed task that the mark did not reach, the
   collector counts the heap holders in unreachable objects. If that
   number is less than the count of the frame, an outside holder
   exists: the task becomes a root, and the mark continues from it
   by rule 2 before the collector reads the next task. One pass over
   the tasks is sufficient: a holder that the continued mark reaches
   is read by the mark, so the mark reaches the task of that holder
   also. The comparison uses the count of the frame, which the
   holders wrote, and the holders that the collector finds, which the
   object descriptions give: two facts with separate origins.
4. **A completed task with heap holders only, all unreachable, ends
   in that collection.** The release of unreachable counted fields
   brings its count to zero, and the frame ends as it does after its
   last holder ends (§70.3). An unobserved failure of that task is
   recorded and delivered by §342 rules 12 to 14.
5. **Each other frame stays a root.** A task that has no completion,
   a task with a waiter, a task in a scheduler state, a generator
   frame, and a frame of a runtime kind (an aggregate, a host
   completion source, a call whose result type holds a handle or a
   generator) are roots as before. A wait that never ends keeps its
   frames (§70.3 rule 6 for unfinished work).
6. **An await gives the same completion (§70.3 rule 4).** No rule
   here ends or changes a completion that a reachable holder or an
   outside holder can read.
7. **The three tiers follow the same rules.** The interpreter has
   reachability code of its own (`codegen/src/interpreter/roots.rs`),
   and it follows rules 1 to 5. The live bytes of the three tiers are
   not equal: the interpreter holds its frames outside the Context,
   and the ship tier counts capacity. Each tier is bounded where
   another is bounded, and the output is equal (§342 rule 2).
8. **The cost follows the garbage.** The step of rule 3 reads only
   unreachable objects that have a description of counted fields, and
   runs only if one completed task is not marked. A collection with
   no such task does no work for this section.

### 348.2 Acceptance

Each test of items 1 to 3 reads the live bytes after the run in the
dev JIT and the ship tier (the method of
`codegen/tests/automatic_collection.rs`), at two repeat counts. The
interpreter runs the same programs, and each test compares the two
figures of one tier (rule 7).

1. **The shapes of the problem are bounded.** The method that
   returns `this`, awaited and not awaited, the function that returns
   its parameter, and the same function with the handle as a Map
   value: the live bytes at two repeat counts are equal. Red at the
   pin: the figures of G7.
2. **The shapes that were bounded stay bounded**: the handle of the own method call with an `i32`
   result, two objects with the task of each other, handles in an
   array and in a Map that the frames hold, a `TaskGroup`.
3. **The wait that never ends keeps its frames** (rule 5): the figure
   of G7 for that shape does not change.
4. **A completion that a holder can read is intact.** Corpus accept
   entries, with a golden, that the stress sweep runs in mode 2 in the
   three tiers. In each, collections run between the completion and
   the read, and the read prints fields of the completion value:
   a. the handle is only in a local of an async function; the object
      that the task returns has no other holder;
   b. the handle is in a field of a reachable object;
   c. the handle is in a `TaskGroup`, and in a `Promise.all`;
   d. the handle is a Map value and an array element of reachable
      containers;
   e. a chain: the completion value of a task with an outside holder
      holds an object that holds the only handle of a second
      completed task, and the program awaits the second after the
      first;
   f. a holder that the mark does not see holds one count. No script
      form and no host form gives it: a host entry returns void
      (§59.1 rule 1), and the mark reads each local, `TaskGroup`, and
      aggregate. A runtime test gives a completed task one count with
      no heap holder, and the task, its completion, and the object of
      the completion stay; the control with no such count ends.
5. **A failed task in an unreachable cycle is reported.** A task
   throws, and nothing awaits it. Its handle is only in an object,
   and a second completed task, whose handle is in the same object,
   returns that object. The program drops the object. The run ends
   with the uncaught-exception trap at the throw position at the
   outermost exit (§342 rule 14), equal in the three tiers and in
   modes 0 and 2. A trap corpus entry. Red at the pin: the run
   prints its last line and exits 0. A task whose own frame held the
   object is not this case: a completed frame holds no parameter and
   no receiver, and the pin reports that failure.
6. **Rule 8.** A test counts the unreachable objects that the step of
   rule 3 reads: zero for a program with no completed task that is
   not marked.
7. The stress sweep of §342.2 item 1 passes in the three tiers, and
   the exit counts agree (§345 rule 4).
8. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
   passes.

### 348.3 Sections this one amends

- §70.3 rule 6: a cycle of completed tasks that only unreachable
  objects hold does not leak under a collection. A cycle of
  unfinished tasks leaks as before.
- §342.5 item 2: closes for completed tasks.
- §94.2 (each scheduler state is a root): a completed task is not a
  scheduler state.
- §116.1 rule 6: the handle of a completed task holds its Error
  object only while the mark reaches the task.

### 348.4 Phase Review

Form.

- `runtime/src/context/completed_tasks.rs` holds the set of completed
  tasks, the read of a completion when the mark reaches its frame, the
  holder count, and the pass of rule 3. `prepare_collection` leaves
  the completed tasks out of three root sets (`async_frames`,
  `completion`, `completion_exception`).
- The holder count reads the same objects and Maps as the release of
  unreachable counted fields, with the same parse of a description
  (`object_fields` in `counted.rs`) and the same arithmetic for a
  counted array. It releases nothing. A count that is too small keeps
  a task; the walk gives no count that is too large.
- A completed frame holds no parameter, no receiver, and no local:
  each tier clears the payload at the completion. So the holder of the
  problem is the completion value only.
- An ending task with an unobserved failure: the collector marks its
  Error object after the release plan, so the record of §342 rule 12
  keeps the object.
- `keep_unended_tasks`: if a listed task is registered at the sweep,
  the collector keeps the frame and its completion and reports an
  internal error.
- The interpreter (`codegen/src/interpreter/roots.rs`) follows rules 1
  to 5 over its own coroutines, and reads the holder count through
  `Context::completed_task_holders`.
- The mark sees each script holder that is not in the heap (a local, a
  `TaskGroup`, an aggregate), so no corpus entry reaches the pass of
  rule 3. Its tests are at the runtime and the interpreter level.

Measured. Live bytes after the run at 1,000 and 1,400 repeats; the
pin is the tree of the contract commit.

| Shape | Dev JIT, pin | Dev JIT | Ship tier, pin | Ship tier | Interpreter, pin | Interpreter |
|---|---|---|---|---|---|---|
| The method returns `this`, awaited | 104,042 and 145,642 | 42 and 42 | 128,016 and 179,216 | 16 and 16 | 72,017 and 100,817 | 17 and 17 |
| The same, not awaited | 104,032 and 145,632 | 32 and 32 | 128,016 and 179,216 | 16 and 16 | 72,012 and 100,812 | 12 and 12 |
| The function returns its parameter | 176,042 and 246,442 | 42 and 42 | 224,016 and 313,616 | 16 and 16 | 144,017 and 201,617 | 17 and 17 |
| The handle is a Map value | 400,042 and 560,042 | 42 and 42 | 592,016 and 828,816 | 16 and 16 | 368,017 and 515,217 | 17 and 17 |
| The five bounded shapes in one program | 448 and 448 | the same | 80 and 80 | the same | 55 and 55 | the same |
| The wait that never ends | 176,014 and 246,414 | the same | 208,016 and 291,216 | the same | 80,014 and 112,014 | the same |

The Map row at the pin is the tree with the count of Map values
switched off.

| Item | Result |
|---|---|
| `a497` to `a501`, three tiers, modes 0 and 2 | equal to the golden; equal at the pin also |
| `t118` at the pin | prints `after`, exit 0, no trap, in the dev JIT, the ship tier, and the stress run of the interpreter |
| `t118` | uncaught-exception, `Error: cycle`, at 18:65, in the three tiers and in modes 0 and 2 |
| Stress sweep, 604 entries | dev JIT 601 of 601, ship tier 599 of 599, interpreter 496 of 496; 485 exit counts compared, no difference; 48.5 s (45 s with 598 entries before this section) |
| Mutation: the mark does not read the completion of a reached task | 6 runtime tests fail; `a497` to `a501` fail in the dev JIT and the ship tier (a runtime error, use of a deleted allocation, or the process stops) |
| The same mutation in the interpreter | `a497` to `a501` fail in the interpreter |
| Mutation: the pass of rule 3 keeps nothing | 5 runtime tests fail; no corpus entry fails |
| The same mutation in the interpreter | `collection_roots_a_completed_task_with_an_outside_holder` fails |
| Rule 8: unreachable objects that the pass reads | 0 with each completed task marked; 1 for the shape of the first row |
| Cost of the tests | each of the six live-byte tests builds two ship programs: 0.4 to 0.8 s in a release build; 1.2 to 1.4 s in a debug build, and 7.4 s for the program of five shapes |
| `perf-gate --gate`, two passes | each criterion met: `a22` ship tier 1.33x and 1.33x, `collect` ship tier 1.15x and 1.15x, `a22` dev JIT 19.88x and 19.90x, `collect` dev JIT 3.68x and 3.69x |

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 7
MINOR. The review read each owner of a count and found none that the
mark or the comparison of rule 3 does not cover, no count that is too
large in the runtime, and no sequence of tasks for which one pass
differs from a repeat. It evaluated the five goldens and the position
of `t118` by hand.

| Finding | Close |
|---|---|
| §70.3 rule 6, §94.2, §116.1 rule 6, §342.5 item 2, and one code comment stated the former roots | each has a note; the comment is corrected |
| The interpreter reads the reachable heap one more time for each collection of a typical async program | open item 1 |
| A call whose result type holds a handle stays a root in the dev JIT and the ship tier, and the interpreter reclaims it | rule 5 states the exclusion; open item 2 |
| The reached set of the interpreter does not include some Context roots | open item 3 |
| `keep_unended_tasks` kept the frame and not its completion | it keeps both; the test reads the object of the completion |
| The holder counter could overflow with a panic in a debug build | a saturating add |
| Acceptance 2 named the figures of G7, and the test compares two counts | the text states what the test compares |

The first run of the full gate on this section failed on one test:
`generated-docs/corpus-index.md` did not hold the six new entries.
The generator wrote it, and the run after that:

```text
gate full 3a74c396eac09f15d3708de139aa63b197256c73 dirty:25 debug 3968/0/5 release 3949/0/5 skips 2/0 clippy 4/10/13 goldens-moved 1 exit 0
```

`tools/hygiene.sh`: exit 0.

### 348.5 Open

1. The interpreter walks the reachable heap one more time in each
   collection where the walk over coroutines does not reach a
   completed task, and again for each task that rule 3 keeps
   (`codegen/src/interpreter/roots.rs`). The dev JIT and the ship tier
   do no such work (rule 8). The stress sweep went from 45 s to 48.5 s
   with six more entries.
2. A completed call whose result type holds a handle or a generator
   (`Promise<Promise<T>>`, `Promise<Promise<T>[]>`) is a root in the
   dev JIT and the ship tier (rule 5), so a cycle through its
   completion is not reclaimed there. The interpreter reclaims it.
   Contrived, MINOR.
3. The holder count of the interpreter takes as unreachable an object
   that only a recorded failure, a pending exception, a callback, or a
   registration reaches. A task that only such an object holds gives
   an internal error. Contrived, MINOR.
4. A cycle of unfinished tasks is not reclaimed (rule 5, §70.3 rule
   6): two tasks that await each other.
5. The filters of the scheduler states in the set of completed tasks
   have no test that fails without them: a frame with a completion
   and a scheduler state did not occur.
6. A task that waits on a host completion source, with its object
   dropped, is not measured (G7).

