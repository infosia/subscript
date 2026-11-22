<!-- §351 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 351. A return into a recursion reaches a poll before it allocates

*(Added 2026-11-22.)* Origin: a review of the poll placement by
another agent, and the measurement G8 of
`specs/tracking/automatic-collection.md`.

Problem, measured at `9a8fa86a`:

1. **The bound of §342 rule 11d does not hold on a chain of
   returns.** A function calls itself and then allocates. With a
   minimum headroom of 64 bytes, a call of depth 1,024 keeps 1,025
   objects and runs no automatic collection before the outermost
   exit, in the ship tier and the dev JIT. The count follows the
   depth, not the module.
2. **The argument counted the descent only.** The entry poll of a
   function in a call-graph cycle serves each call. After the deepest
   call returns, no poll is on the path of the continuations.
3. **The verifier does not see it.** It checks the cycles of one
   function and the entry polls. A lowered module with no poll after
   a recursive call passes.
4. **Three more shapes have the same gap**: a recursion through a
   function value, a cycle that a direct call closes after an
   indirect call, and a handler that allocates when an exception
   returns through the frames.

### 351.1 Rules

Terms. The **call graph with indirect edges** has an edge for each
direct call, and an edge from each function with an indirect call, or
with a runtime operation that takes a function value, to each function
whose address is taken. A **cycle** is a strongly connected component
of that graph that has an edge inside it. The **continuation** of a
call is the path from the instruction after the call.

1. **A poll after a call into the cycle of the caller (rule 11e of
   §342).** For each call in a function of a cycle whose possible
   callee is in that cycle: if a path of the continuation reaches an
   instruction that can allocate, or a suspension, before it reaches a
   poll, the placement puts one poll directly after the call.
   *(Amended 2026-11-22, §352 rules 1 to 4: the graph comes from a
   total fact of each instruction; a resume or a close of a generator
   and a foreign call are calls of this rule.)* A path
   follows the handler edges of later instructions and ends at a
   return, a trap, or an unreachable point.
2. **A handler of a function of a cycle has a poll (rule 11f).** In
   each function of a cycle that can allocate, each handler block
   whose path can allocate before a poll gets one poll after the
   leading instructions that take the exception.
3. **A direct call to a function with an entry poll counts as a poll**
   in the scan of rules 1 and 2, if the callee is not a generator and
   not async: a call of those creates a frame and does not run the
   entry block. So a function that calls itself two times and then
   allocates gets one poll, after the second call. *(Amended
   2026-11-22, §352 rule 5: a resume of a generator does not count as
   a poll.)*
4. **A function outside each cycle gets no new poll.** A frame of it
   is on the stack one time at most.
5. **The entry poll rule uses the same graph.** A function that can
   allocate and is in a cycle of the graph with indirect edges has an
   entry poll (§342 rule 11b named the direct cycle and the functions
   whose address is taken; the set does not shrink).
6. **The bound.** Each path through LIR control flow reaches a poll,
   or the outermost exit, after a count of allocating instructions
   that only the module bounds, for direct calls, indirect calls,
   callback operations, normal returns, exceptional returns, and the
   resumes of one drain (rule 12). *(Amended 2026-11-22, §352 rules 8
   and 9: also for the resumes and the closes of generators.)* The
   module documentation of the placement pass states the argument
   with the chain of returns.
7. **Not in the bound**, each recorded in §351.4: a recursion in
   which the host calls an export from inside a foreign call; the
   allocations of one runtime operation (§345 rule 3). *(Amended
   2026-11-22, §352: a value that passes up through nested generators,
   and a callback that the host calls from a foreign call, are in the
   bound.)*
8. **The verifier checks rules 1, 2, and 5** by a derivation of its
   own: a backward fixpoint over block suffixes for "an allocation is
   reachable with no poll", against the forward scan of the placement.
   A module with one of those polls removed is rejected while its
   entry polls are present.
9. **`Coerce` of a reference class to its own nullable type cannot
   allocate.** The fact table of §342 rule 10 stated "can allocate"
   for it. With the correct fact, rule 1 adds no poll to a tree-shaped
   recursion.
10. **A value that the continuation reads is rooted at the poll**, as
    at each poll: the root storage follows the liveness of the
    finished LIR. No rule of the lowerings changes.
11. **An automatic collection stays not observable** (§342 rule 2):
    no golden of the corpus moves.

12. **The drain of the scheduler serves the collection flag between
    two jobs (rule 11g of §342).** *(Added 2026-11-22, after the Phase
    Review.)* The continuation of an `await` runs as a resume, and one
    drain runs each job that becomes ready in it, so a chain of
    resumes of an async recursion had no poll between its frames.
    Between two jobs the state of each script activation is in a
    frame, a ready job, or a waiter, and each of them is a root. The
    drain tests the flag before it takes the next job, after the first
    job of the drain. This is a poll and not an outermost exit: the
    exit count does not change, and a collection there records a
    failure and does not trap (§342 rule 12). A drain with a budget of
    zero runs no job and no poll. The interpreter has the same poll in
    its drain. The call graph has an edge for an `await` of a direct
    async call.

### 351.2 Acceptance

1. **Collection on the return path.** The fixture of the problem at
   depths 256 and 1,024, automatic mode, minimum headroom 64 bytes,
   sampled before the outermost exit, in the dev JIT and the ship
   tier: an automatic collection runs in each call, and the objects
   over the base are under a bound that the test derives from the
   object size and the headroom, equal for the two depths. Red at the
   pin: 257 and 1,025 objects, no collection. The test does not use
   mode 2: a collection at each poll collects on the descent and
   passes for a wrong reason.
2. **The other shapes**, in the same test form: two functions that
   call each other; a recursion through a function value; a cycle
   that a direct call closes after an indirect call, where only the
   continuation of the direct call allocates; a handler that
   allocates with a throw from the bottom. Red at the pin for each.
   Control: a recursion whose continuation does not allocate has no
   new poll and keeps one object.
2a. **Two more shapes of the placement**: a cycle that exists only
   through a runtime operation with a function-value operand
   (`forEach` with a callback in a variable), and a continuation that
   reaches a suspension before an allocation (a generator that calls
   down through a function value and then yields). Each has a unit
   test of the placement and a removal test of the verifier, and the
   first has the test of item 1. *(Amended 2026-11-22, §351.4 item
   6.)* The first runs the test of item 1 at depths 128 and 256.
2b. **Rule 12.** An async recursion that allocates five objects for
   each level, its held form, and a form that suspends before it
   calls down, at depths 256 and 1,024, automatic mode, minimum
   headroom 64 bytes, in the dev JIT and the ship tier: the objects
   before the exit of the drain are under the bound of item 1. Red
   with no poll in the drain: 1,281 and 5,121 objects. A level that
   allocates one object of the size of its frame does not set the
   flag (the release of the child frame takes the same bytes from the
   requested count), so that fixture shows nothing. Runtime tests:
   the drain collects between two jobs in mode 1; a budget of zero
   gives no collection; the exit count does not change. Roots: three
   tasks that hold a managed value over two suspensions, mode 2, in
   the three tiers, output equal to mode 0.
3. **Verifier.** For each shape of items 1, 2, and 2a: the lowered module
   with only the new poll removed is rejected, with its entry poll
   present. A recursion with no allocation has no poll and passes;
   the same recursion with one allocation after the call and no poll
   is rejected (the firing control).
4. **Roots.** A recursion that reads a managed result of the call
   after the new poll (an object, a string, an array, and an object
   that a handler reads), with mode 2, in the three tiers: the output
   equals the output of mode 0.
5. **Rule 9.** The fact of the coercion has a test. The emitted C of
   `tree`, `callbacks`, `sort`, `particles`, `queen`, `fib-recursive`,
   and `collect` has the same count of polls as at the pin.
6. **Cost.** `perf-gate --gate` meets each criterion. The LIR snapshot
   gets the lines of the new polls and no other change.
7. The stress sweep of §342.2 passes in the three tiers.
   `tools/gate.sh full` passes. No corpus golden moves.

### 351.3 Sections this one amends

- §342 rule 11: rules 11e, 11f, and 11g here (rules 1, 2, and 12);
  rule 11d has the bound of rule 6 here. §342 rule 10: rule 9 here.
- §345 rule 3: the bound has the scope of rule 6 here.

### 351.4 Open

1. A recursion that passes through the host is not in the call graph:
   a foreign call, then an export call from the host. Contrived.
   *(Amended 2026-11-22: §352 rule 1 gives a foreign call an edge to
   each function whose address is taken, so a registered callback is
   in the graph. The export form stays here; the ship tier traps a
   host entry inside a host entry, §223.)*
2. Closed by §352, which measures the path (its acceptance 2). A
   value that a chain of nested generators passes up crosses one
   suspension for each generator, with no poll between them.
3. The interpreter runs each level of a held async chain on the host
   stack. The held form of acceptance 2b ends the stack of a 2 MiB
   test thread at depth 32 in a debug build, so the interpreter test
   of rule 12 uses depth 16. The cause is not investigated.
4. The dev JIT gives no read of the size of a coroutine frame to a
   test. The bound of acceptance 2b uses the capacities that the ship
   tier reports (48 bytes for the frame and for the object) as a
   premise for the dev JIT.
5. The bound counts instructions. A level that releases as many bytes
   as it requests does not set the flag (§346), so its objects stay
   until a later poll finds the flag set, or until the outermost exit:
   the fixture of the review with an `await`, 257 and 1,025 objects.
   The bytes that the Context holds stay under the limit of §346.
6. *(Added 2026-11-22.)* The ship program of a debug test build ends
   a main-thread stack of 1 MiB in the `forEach` shape of acceptance
   2a. Measured on `x86_64-pc-windows-msvc` at `524edac8`: depth 512
   passes, and depth 1,024 exits with `0xc00000fd`. Each level has one
   frame of the runtime `forEach`. The test links the runtime archive
   of its own profile, and the release build passes at depth 1,024.
   So the test runs that shape at depths 128 and 256 on each
   host and in each profile. The bound does not depend on the depth,
   and with no poll the count is the depth, so the smaller depths
   show the same fact. The stack that one level takes is not
   measured.

### 351.5 Phase Review

Form.

- `codegen/src/lir/collection_poll.rs`: the call graph with indirect
  edges, with an edge for an `await` of a direct async call; the entry
  polls from that graph; a forward scan of each continuation; the
  polls after a call and in a handler. The sites are found on one
  snapshot and inserted from the last index.
- `codegen/src/lir/verify_collection_poll.rs`: a graph of its own
  with Tarjan components, a least fixpoint over block suffixes, and
  "has an entry poll" read from the finished entry block.
- The drain poll: `async_drain` tests the flag before it takes the
  next job, after the first job (`runtime/src/context/async_scheduler.rs`);
  `drain_ready` of the interpreter does the same
  (`codegen/src/interpreter/checkpoint.rs`). The poll is the function
  behind `subscript_rt_collect_poll`.
- A drain runs with a script frame under it in one case: the dev JIT,
  a foreign function that calls the step function (§223 rule 6). The
  values of that frame are in root slots at a foreign call, and the
  jobs of that drain reached their own polls in the same state before
  this section.
- The language has no virtual call: each call form is in the graph.

Measured. Objects over the base before the outermost exit, automatic
mode, minimum headroom 64 bytes, depths 256 and 1,024.

| Shape | At the pin, both tiers | Ship tier | Dev JIT |
|---|---|---|---|
| The fixture | 257 and 1,025 | 2 and 2 | 2 and 2 |
| Two functions that call each other | 257 and 1,025 | 2 and 2 | 2 and 2 |
| A recursion through a function value | 257 and 1,025 | 2 and 2 | 2 and 2 |
| A direct call that closes a cycle after an indirect call | 129 and 513 | 3 and 3 | 3 and 3 |
| A handler that allocates | 259 and 1,025 to 1,027 | 6 and 3 | 6 and 6 |
| A cycle through `forEach` with a callback in a variable | 256 and 1,024 | 1 and 1 | 1 and 1 |
| A continuation that does not allocate (control) | 1 and 1 | 1 and 1 | 1 and 1 |
| An async recursion, five objects for each level, with no poll in the drain | 1,281 and 5,121 | | |
| The same with the poll | | 1 and 1 | 2 and 2 |
| Its held form; its form that suspends first | | 1 and 6 | 2 and 2 |

| Item | Result |
|---|---|
| Automatic collections in the call, ship tier, the fixture | 0 at the pin; 85 and 341 |
| Poll sites of `tree`, `callbacks`, `sort`, `particles`, `queen`, `fib-recursive`, `collect` | 2, 2, 1, 1, 0, 0, 2 at the pin and after; `tree` has 4 with rule 1 and the former fact of the coercion |
| Poll sites over the accept corpus | 1,535 at the pin; 1,534 with rule 9; 1,567 with each rule |
| LIR snapshot | 18 `CollectionPoll` lines added, none removed |
| Rule 10, mode 2 | a result of the call (an object, a string, an array) and an object that a handler reads: output equal to mode 0 in the three tiers; three tasks that hold a value over two suspensions: the same |
| Stress sweep, 614 entries | dev JIT 611 of 611, ship tier 609 of 609, interpreter 499 of 499; exit counts equal; automatic collections of the dev JIT in mode 2 from 31,962 to 33,596 |
| `async-cost`, two runs with the drain poll and two with none, median ms | `settled-awaits` 15.641 (spread over the limit of the tool) and 14.304 against 14.629 and 14.427; `held-handles` 4.261 and 4.227 against 4.186 and 4.232; `deep-chains` 9.492 and 9.507 against 9.869 and 9.462 |
| `perf-gate --gate`, two passes | each criterion met: `a22` ship tier 1.33x and 1.33x, `collect` ship tier 1.14x and 1.15x |
| Drain tests of the runtime | a budget of 0: no collection; a budget of 3 in mode 2: two poll collections and one exit collection; the exit count does not change |
| Cost of the tests | `codegen/tests/return_path_polls.rs`: about 2 s in a release build, 4 s in a debug build |

Phase Review, first (a subagent with no context): no CRITICAL, 2
MAJOR, 3 MINOR. The review followed eleven shapes of a chain of
returns through the placement; ten were covered.

| Finding | Close |
|---|---|
| MAJOR: the return chain of an async recursion had no poll; the continuation of an `await` is a resume, and one drain ran each resume with no poll between jobs | rule 12 |
| MAJOR: "or a suspension" of rule 1, and the edge of a runtime operation with a function value, had no test that can fail | acceptance 2a; each test is Red with that part removed |
| A direct call that creates a generator counted as a poll, in the placement and in the verifier | rule 3 |
| §342 rule 11 and §345 rule 3 had no note, and §342 had no rules 11e and 11f | the notes and rules 11e to 11g |
| A test comment stated a former behaviour | deleted |

Phase Review, second, of the drain poll and the changes after the
first review: no CRITICAL, 1 MAJOR, 3 MINOR. The review found no value
that a collection at the drain poll frees and that the drain or a job
reads after it, and no new class of risk in the nested case.

| Finding | Close |
|---|---|
| MAJOR: no test failed with the poll of the interpreter removed | a test of the interpreter drain with a budget, and a test of an async chain; each is Red with the poll removed |
| The module documentation of the placement stated that the drain has no poll | it states rule 12 |
| The bound of the async tests assumed that the limit is the headroom; the limit follows the live bytes, which include the frames that wait | the bound has the frames in it, and the test asserts the condition under which the flag is set |
| One `// SAFETY:` comment for two blocks, and an `assert!` in an `extern "C"` function of a test | corrected |

`tools/gate.sh full` on the tree before this record:

```text
gate full cc4ecf7ccea80eeaa8244ce90a2d9193f87a1db0 dirty:12 debug 4072/0/5 release 4053/0/5 skips 2/0 clippy 4/10/13 goldens-moved 1 exit 0
```

`tools/hygiene.sh`: exit 0.

