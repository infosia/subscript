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
   completion source) are roots as before. A wait that never ends
   keeps its frames (§70.3 rule 6 for unfinished work).
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
2. **The shapes that were bounded stay bounded**, with the same
   figures as G7: the handle of the own method call with an `i32`
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
