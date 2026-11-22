# Automatic collection — trial on the branch `gc-auto`

Status: **§342 to §346 COMPLETE, and on `main` at `6169b0e5`
(2026-11-22).** The measurements of the implementation and the Phase
Reviews are in `compiler.md` §342.4a, §343.4, §344.5, §345.5, and
§346.4. The prototype branches did not land, and they are deleted.

## Question and owner decisions

The trial asks if the language can collect automatically and remove
`Context.free`. The rule that the prototype contradicts is design
invariant 2 (no implicit GC). The trial does not change the decision of
2026-11-17 in `memory-safety-study.md` on `main`.

The owner decided these points on 2026-11-21:

1. An unobserved failure that an automatic collection finds is recorded
   and reported at the host boundary, if the cost is acceptable.
2. The host can switch automatic collection off. The default is on.
3. The removal of `Context.free` is decided after the safepoint
   measurements.
4. After G0, the trial goes on to the contract.
5. A detection collection at the host boundary runs on a schedule of
   exit counts with a backoff (§342 rules 14 and 15), not at each exit.
6. `Context.free` stays. The round G4 does not run. W001 goes in a
   section of its own after §342.
7. The documents state the host's control of the pause first, then
   three levels (automatic, host-timed, manual). Invariant 2 has the
   same form (§342 rule 4a).

## Rounds

| Round | Content |
|---|---|
| G0 | Measurement. Lands nothing. |
| G1 | Contract: invariant 2, Q6, Q7, lifetime, failure delivery. Red corpus entries. |
| G2 | Threshold, pending flag, host-boundary service, host policy API. |
| G3 | LIR poll, three lowerings, stress in the gate. |
| G4 | Removal of `Context.free` (owner decision 3). |
| G5 | Phase Review. |

G1 to G3 are `compiler.md` §342 and its rounds (§342.4). If `main`
takes §342 first, the section gets a new number at the merge.

## G0a — root visibility and trigger equality, at `ca3fc9ed`

Prototype: commit `e861b36b` on `gc-auto-proto`. A LIR pass puts one
poll at each function entry and one before the terminator of each
backedge source block, if `SUBSCRIPT_GC_POLL=1` is set at compile time.
A poll is an out-of-line runtime call. An allocation sets a pending
flag when live bytes exceed the threshold, and never collects. The
threshold after a collection is the larger of 2 × live bytes and live
bytes + 1 MiB. `SUBSCRIPT_GC_STRESS=1` makes each poll collect.

### Stress: each poll collects

| Set | Tier | Equal to the run without polls |
|---|---|---|
| 402 accept + 97 trap entries | dev JIT | 494 of 499 |
| same | ship C | 494 of 499 |
| same | interpreter | 495 of 499 |
| 78 native, hook, and file entries (`golden.rs`) | dev JIT and ship C | 78 of 78 |

No live reference was collected in any tier. The differences:

| Entry | Tier | Difference |
|---|---|---|
| `a22-matrix-propagation` | dev JIT, ship C | No result in 120 s: 10,000 live nodes, and each backedge runs a full collection. |
| `t98`, `t99`, `t100`, `t107` | all three | The trap position moves to the poll that released the last owner of an unobserved failed task. Message and stdout are equal. The three tiers agree on the new position. |

The interpreter did not run the 93 entries that need a native library.

### Tree without `free` (threshold mode, `COUNT` 30)

| Tier | Checksum | Collections | Maximum live bytes after a collection | `tree` with `free`, no polls | Without `free`, polls |
|---|---|---|---|---|---|
| ship C | 3932130 | 44 | 2,097,104 | 112 ms | 168 ms |
| dev JIT | 3932130 | 44 | 2,097,104 | 463 ms | 589 ms |
| interpreter | 3932130 | 44 | 2,097,104 | 27.7 s | 20.9 s |

Each time is the middle of three runs. The orchestrator ran the dev
JIT row again: 3932130, 44 collections, 524 ms.

### Trigger equality

The total poll count is equal in the three tiers for each program
measured. The poll where a collection runs is not equal in general:

- The ship tier counts the size-class capacity as live bytes. The dev
  tier counts the exact payload. They agree only if each payload size
  is a class size (the tree: 16 B).
- The interpreter and the dev JIT differ in live bytes at the same poll
  on `a338` and `a214`. The cause is not traced.

Consequence: a rule that makes the report of a failure depend on the
poll where a collection runs is not byte-exact across the tiers. The
delivery rule must not depend on that poll.

### Poll cost with an out-of-line call (upper limit)

| Workload | Tier | No polls | Polls | Ratio |
|---|---|---|---|---|
| `fib-recursive` | dev JIT | 8.62 ms | 12.67 ms | 1.47 |
| `fib-recursive` | ship C | 7.1 ms | 15.8 ms | 2.2 |
| `sort` | dev JIT | 36.8 ms | 55.0 ms | 1.50 |
| `sort` | ship C | 22.7 ms | 42.1 ms | 1.86 |

The cost with an inline flag test is not measured (G0b).

## Inventory at `ca3fc9ed` (source read, not run)

### Host-held memory that only explicit collection keeps valid

- A host that keeps an array data pointer, a string view, or a pointer
  to a struct passed as script memory after the foreign call returns.
  The contract ends the borrow at the return (`collisions.md` Q4 and
  Q15; `compiler.md` §28, §323 rule 1). The C tutorial does not state
  it.
- A host that uses a pointer passed to an async completion function
  until the completion (`compiler.md` §323.4 item 4, open).
- `subscript_rt_worker_poll`, `subscript_rt_worker_inbox_wait`, and
  `subscript_rt_worker_inbox_poll` return a managed allocation that no
  root set holds. The header states no lifetime.
- A host entry carries no managed value in either direction. Callback
  userdata and open registrations are roots.

### Runtime code that holds a reference in a Rust local while script code runs

The array operations with a literal lambda lower to script-level LIR
loops, and the root plan covers them. The sites below are the
function-value paths and the loops in Rust (`runtime/src/arrops.rs`,
`runtime/src/assocops.rs`). The interpreter roots each of them with a
snapshot. The native tiers do not.

| Site | Reference with no root |
|---|---|
| `filter` | the element, between the callback and the push |
| `find`, `findLast` | the element, between the callback and the copy |
| `Map.groupBy` | the element copy |
| `sort` | the scratch copies (`compiler.md` §323.4 item 2) |
| `reduce`, callback message string | the argument, before the callee stores its parameters |

An explicit `Context.collect()` in the callback reaches the same sites
today. No corpus entry has these shapes, so the stress run did not
reach them.

## G0b — inline poll, failure delivery, runtime probes

Prototype: commit `613b0a64` on `gc-auto-proto`. Host: aarch64, macOS.
Each time is the middle of three release runs. The dev JIT figure times
`main` only. The ship C figure times the whole process. Ship runs of
about 10 ms vary by up to 60% between passes.

### Failure delivery that does not depend on the poll

Rule of the prototype:

1. A poll collection does not trap. It records the failure and keeps
   the error object reachable.
2. An explicit `Context.collect()` reports as before, and reports the
   recorded failures at its call position. The lowest task id is first.
3. At each outermost exit from script code, if a recorded failure or a
   failed unobserved task exists, one collection runs and the result is
   reported at the throw position.
4. `await` and the last-owner release report as before.

| Run (498 entries per tier, `a22` not run) | Result |
|---|---|
| Stress against the run without polls | 495 of 498 equal in each tier |
| Threshold against the run without polls | the same 495 of 498 |
| Stress against threshold | 1,494 of 1,494 equal |

The delivery does not depend on the poll where a collection runs. The
three entries that change are `t97`, `t98`, and `t107`: an `await` ends
a scheduler step, each step is an outermost exit, and that exit reports
the failure at the throw position (`11:55`) before the program reaches
its `Context.collect()` (`19:3`, `20:3`, `23:3`). Message and stdout
are equal. The three tiers agree.

Cost of rule 3 on the ship tier, rule off against rule on:
`held-handles` 7.11 and 7.10 ms, `deep-chains` 12.50 and 12.61 ms,
`settled-awaits` 16.36 and 16.34 ms, `callbacks` 289.3 and 288.9 ms.

The interpreter has no shared form for its exit points. The prototype
places them by hand to match the native tiers.

### Poll cost with an inline flag test, no collection

The poll is one load of a Context byte and one branch. The call is on
the cold path. The headroom is set so that no collection runs.

| Workload | dev JIT | ship C |
|---|---|---|
| `fib-recursive` | 1.05 | 1.23 |
| `sort` | 1.13 | 1.16 |
| `callbacks` | 1.27 | 1.94 |
| `particles` | 1.10 | 1.49 |
| tree without `free` | 1.01 | 1.09 |
| `fib-loop` | 1.76 | 1.02 |
| `mandelbrot` | 2.51 | 1.03 |

The cause of the dev JIT figures on loops with no call, and of the ship
figures on `callbacks` and `particles`, is not traced (G0c).

A variant puts an entry poll only in a function that is in a call-graph
cycle, has its address taken, or is async or a generator. The LIR
carries the facts for that decision. `fib-recursive` and `sort` do not
change, because their hot functions are recursive.

### Cost with the default threshold

| Workload | dev JIT | ship C |
|---|---|---|
| tree without `free`, against itself with no polls and no collection | 1.23 | 2.32 |
| `callbacks` | 1.93 | 6.09 |
| `held-handles` | 1.05 | 1.40 |
| `collect` | 1.05 | 1.12 |
| generators, `queen`, `tree` with `free`, other async | 0.96 to 1.06 | 0.96 to 1.04 |

The run with no polls never collects, so the first two rows contain the
collection work.

### Runtime probes (function-value callbacks)

The entry poll sees each parameter: both native tiers store the
parameters into the shadow slots before the entry block.

| Probe | Interpreter | dev JIT | ship C |
|---|---|---|---|
| `filter`, `find`, `findLast`, `Map.groupBy`: the callback empties the receiver | correct | runtime error "use of a deleted allocation", or a wrong value | wrong value |
| `sort`: the comparator overwrites elements | correct | wrong value | wrong value |
| `reduce` with a string accumulator | correct | correct | correct |

The native results are the same under stress polls and under an
explicit `Context.collect()` in the callback with no polls. The second
case is a defect at `ca3fc9ed` that needs no automatic collection.
The orchestrator read the recorded outputs of the `find` probe:
interpreter `find 1` / `findLast 32`; ship C with an explicit collect
`find 0` / `findLast 2055278416`.

### Not measured

- `bound-call` and the 93 entries that need a native library, in this
  round.
- The MSVC form of the cold-path hint.

## G0c — polls only where an allocation can occur

Prototype: commit on `gc-auto-proto` after `613b0a64` (see the branch
log). Only an allocation sets the pending flag, so a poll in code that
cannot allocate serves nothing.

### Cause of the G0b poll cost

| Figure | Cause |
|---|---|
| ship C `callbacks` 1.94 | The poll branch with its cold call stops vectorization of the `reduce` loop: 26 NEON `.4s` instructions with no polls, 0 with polls. The entry poll also gives each callee a `Call` trap, so each call site gets a trap test. |
| ship C `particles` 1.49 | The cold call can write memory that the root slot reaches. The loop loads the root slot, the length, the data pointer, and the stride again in each iteration. |
| dev JIT `mandelbrot` 2.51 | The function gets a frame, and the loop spills its f64 values: 5 loads and 4 stores per iteration. A second trap test runs on the hot path, because the poll carries a `Call` trap. |
| dev JIT `fib-loop` 1.76 | No spill. A loop of about 4 instructions gets the flag test, the trap test, and copies of the Context pointer. |

### Placement P

- A function "can allocate" if it has an instruction that can allocate,
  or a call to a function that can allocate. An indirect call can
  allocate if any function with its address taken can. A foreign call
  always can.
- A backedge poll exists only if a block of the natural loop can
  allocate.
- An entry poll exists only if the function can allocate, and it is in
  a call-graph cycle, or has its address taken, or is async, or is a
  generator.

The LIR does not carry the allocation fact of an intrinsic or of a
builtin method. The prototype uses hand lists by operation name. The
lists are not verified against the runtime code. A landed form needs
the fact in the operation table (core principle 8).

Paths that placement P does not cover:

- A host-called entry that allocates in straight-line code. No poll
  runs, and live bytes grow with each host call. One service at each
  outermost exit closes it.
- A generator resume and an async step are not verified to reach a
  poll.

### Poll sites

| Set | G0b placement | P |
|---|---|---|
| Accept corpus, 480 entries, 4,358 functions | 5,034 sites | 1,462 sites; 206 entries have none |
| `fib-recursive`, `fib-loop`, `mandelbrot` | 3, 5, 7 | 0, 0, 0 |
| `sort`, `callbacks`, `particles` | 9, 11, 8 | 1, 2, 1 |

Emitted C grows by 0.1% to 6.2% on six large entries with placement P
(`a423`: 339,130 to 348,626 bytes; `a405`: 87,616 to 93,074).

### Cost with placement P (ratio against no polls)

| Workload | dev JIT, no collection | ship C, no collection | dev JIT, 1 MiB headroom | ship C, 1 MiB headroom |
|---|---|---|---|---|
| `fib-recursive` | 1.00 | 1.01 | 1.00 | 1.01 |
| `sort` | 1.00 | 1.01 | 1.12 | 1.23 |
| `callbacks` | 1.00 | 1.01 | 1.45 | 3.90 |
| `particles` | 0.98 | 1.00 | 1.00 | 1.06 |
| tree without `free` | 1.00 | 1.01 | 1.17 | 2.29 |
| `fib-loop` | 1.00 | 1.01 | 0.99 | 0.99 |
| `mandelbrot` | 1.00 | 1.00 | 1.00 | 1.00 |

The run with no polls never collects, so each figure over 1.0 in the
last two columns is collection work.

### Minimum headroom

| Workload | 1 MiB | 16 MiB | No polls |
|---|---|---|---|
| `callbacks`, ship C | 23 collections, 185 ms | 7 collections, 90 ms | 47.5 ms |
| tree without `free`, ship C | 44 collections, 145 ms | 3 collections, 62.6 ms | 62.6 ms |
| `particles`, ship C | 2 collections, 82.7 ms | 0 collections, 77.7 ms | 77.8 ms |

The reference for the tree without `free` never releases memory. The
`tree` workload with `Context.free` and no polls takes 108 to 112 ms
on the ship tier (G0a, G0b). `callbacks` keeps about 6 ms for each
collection with about 4 MiB live; the cause is not traced.

### Stress with placement P

Each tier: 495 of 498 equal to the run without polls. The differences
are `t97`, `t98`, and `t107` (the delivery rule of G0b). The 78 native,
hook, and file entries: 78 of 78 equal in the dev JIT and ship C.

## G0d — a detection collection at each exit

Prototype `3304e309` on `gc-auto-proto`, dev JIT, by the orchestrator.
The program builds a list of 200,000 nodes, puts a failed handle in a
live object, runs 2,000 iterations of `await Context.suspend()`, then
awaits the handle in a `try`. Each run prints `caught` and
`2000 200000`.

| Configuration | Time, three runs |
|---|---|
| No polls | 0.02 s, 0.02 s, 0.02 s |
| Polls, no collection at the outermost exit | 0.02 s, 0.02 s, 0.02 s |
| Polls, the G0b rule 3 | 20.53 s, 20.74 s, 20.87 s |

The G0b rule 3 runs one full collection at each outermost exit while
a failed unobserved task exists: about 10 ms for each step here. With
`await` of an async function that completes at once in place of
`Context.suspend()`, the three configurations read 0.02 s, 0.02 s,
and 0.06 s. The ship tier was not measured: the driver did not link
the runtime library from that directory.

## At the merge

- The repository description on the hosting site states "no implicit
  GC". The owner decided the new text on 2026-11-22: "Embeddable
  statically-typed TypeScript-subset for native apps. C-ABI data
  layout, zero-copy C interop, automatic GC with host-controlled
  pauses. JIT hot reload in dev, ships as AOT C." The owner sets it.
- If `main` has a §342, this section takes the next free number, and
  each citation of §342 in the code and the documents moves with it.

## Next

- `compiler.md` §342.5 holds the open items.
- §345 closed the items of the two reviews of `aef4cdd3` by another
  agent: the roots of a runtime operation across a function-value
  callback, the scope of §342 rule 11d, and the exit count of the ship
  tier.
- §346 changed the trigger after `perf-gate` failed on `collect`: the
  figure of 1.18x to 1.22x that §344.4 recorded was against a C
  reference of 42 to 44 ms. With the reference at 31.4 ms the same
  ship time was 1.59x.
- §348 reclaims a completed task that only unreachable objects
  hold: the two typical shapes of G7. The evidence is in `compiler.md`
  §348.4. A cycle of unfinished tasks stays (§348.5 item 4).

## G6 — benchmark comparison against `main`, and the causes

Host: aarch64, macOS. `main` is `ca3fc9ed`; the branch is `aef4cdd3`.
Release builds of `cross-language --only <workload>` (median of 11
timed runs) and of `perf-gate --gate`, run in turn, two passes.

| Subject | `main` | Branch |
|---|---|---|
| `callbacks` ship tier | 37.09, 37.76 ms | 88.49, 86.05 ms |
| `callbacks` dev JIT | 257.9, 259.1 ms | 338.9, 334.6 ms |
| `collect` ship tier | 38.61, 37.49 ms | 53.16, 53.42 ms |
| `collect` dev JIT | 121.4, 121.1 ms | 121.2, 120.8 ms |
| `perf-gate` `a22` ship tier | 1.35x, 1.33x | 1.44x, 1.43x |
| `perf-gate` `collect` ship tier | 0.87x, 0.85x | 1.22x, 1.18x |
| `fib-recursive`, `fib-loop`, `mandelbrot`, `primes`, `sort`, `tree`, `queen`, `particles`, both tiers | equal in the noise of the two passes | |

Prototype for the causes: the branch `gc-auto-proto2`. It does not
land.

### `callbacks`: the mark reads a buffer of scalars

One process ran 98 automatic collections (7 for each iteration of the
workload). `main` runs none: it keeps about 140 MB of garbage.

| Tier | Mark | Sweep | Words read | Lookups that hit | Live bytes before and after |
|---|---|---|---|---|---|
| ship | 7.3 ms | 17 µs | 524,294 | 2 | 28.2 MB, 4.19 MB |
| dev JIT | 8.9 ms | 17 µs | 524,294 | 2 | the same |

The mark reads the 4 MB data buffer of the live `i32[]` word by word,
because `class_holds_no_handle` is true for a string only. 7 × 7.3 ms
is the 51 ms that the ship tier lost.

With no read of a buffer whose element size is under 8 bytes, the 98
collections cost about 2 µs each:

| Workload | `main` | Prototype |
|---|---|---|
| `callbacks` ship tier | 37.46, 37.14 ms | 36.62, 36.69 ms |
| `callbacks` dev JIT | 257.2, 257.6 ms | 256.7, 256.2 ms |
| `particles`, `sort`, `tree`, both tiers | | equal in the noise |

An `f64[]` has an element size of 8, so that rule does not cover it.
Probe, ship tier, a live 4 MB `f64[]` and garbage in a loop, 12
automatic collections: the mark took 13.5 ms for each collection, and
about 0 with no read of any array data buffer. The run took 197 and
169 ms (minimum of two passes) against 66.5 and 65.2 ms.

### `collect`, ship tier: automatic collections between the explicit ones

| Tier | Explicit | Automatic | Live after an explicit collection | Threshold | Peak of a round |
|---|---|---|---|---|---|
| ship | 105 | 90 | 14.16 MB | 30.94 MB | about 33 MB |
| dev JIT | 98 | 0 | 7.02 MB | 23.8 MB | 16.6 MB |

The ship tier counts the size-class capacity. The workload requests
one byte over each class size, so the capacity is 2.02 times the
request. One automatic collection costs 2.8 ms of mark and 0.35 ms of
sweep. With mode 0 the same binary reads 37.05 and 38.57 ms, so the
polls, the flag of an allocation, and the explicit path cost nothing
that the measurement shows.

### `a22`, ship tier: a poll in a loop that cannot allocate

No collection runs in `a22`. The backedge of the loop in `propagate`
(1,000,000 iterations) has a poll, because `multiply` states "can
allocate". Two instructions give that fact and only make values: an
`ArrayLiteral` with a `FixedArray<f32, 16>` result (a compound literal
in the emitted C), and an `AllocateClass` of a `@ValueType` class with
an address result (a stack temporary). With both as "cannot allocate",
the loop has no poll: 8.42 and 8.37 ms with the poll, 7.91 and 7.89 ms
with none (minimum of 40 runs, two passes).

The `perf-gate` run of this round was void after its first pass: a
load from outside the round gave spreads of 14% to 360%.

## G7 — async frames under automatic collection

Measured 2026-11-22 at `5bbb167c`, with no change of the tree. The
question: does a typical async program keep memory that no root
reaches (§342.5 item 2)?

Method. Each program repeats one shape N times, and its last step is
`Context.collect()`. A harness outside the tree reads the live bytes
of the Context after the run: the dev JIT with memory accounting, and
the ship tier through a post-run hook that calls
`subscript_rt_ctx_collect` one more time. The dev figure is requested
bytes and the ship figure is capacity, so each tier compares with
itself only. The default mode ran no automatic collection at these
counts; one shape at N = 100,000 ran one, and its figure did not
change. The interpreter was not measured.

| Shape | Dev, N = 1,000 and 10,000 | Ship, the same | Result |
|---|---|---|---|
| A loop awaits an async function that allocates | 40, 44 | 16, 16 | bounded |
| An object stores the handle of its own method call; awaited; dropped | 42, 46 | 16, 16 | bounded |
| The same, never awaited | 15, 15 | 16, 16 | bounded |
| The same, the method suspends one time | 42, 46 | 16, 16 | bounded |
| Two objects, each holds the task whose frame holds the other | 42, 46 | 16, 16 | bounded |
| The handles are in an array or a Map that the frames hold | 42, 46 and 38, 40 | 16, 16 | bounded |
| A `TaskGroup` local, joined | 42, 46 | 16, 16 | bounded |
| A module array of objects, each with its task | 82, 86 | 64, 64 | bounded |
| **The method returns `this`; the object stores the handle** | 104,042 and 1,040,046 | 128,016 and 1,280,016 | 104 bytes for each iteration (dev) |
| **An async function returns its parameter; the parameter stores the handle** | 176,042 and 1,760,046 | 224,016 and 2,240,016 | 176 bytes for each iteration (dev) |
| Two methods of one object, each awaits the handle of the other | 176,014 and 1,760,014 | 208,016 and 2,080,016 | 176 bytes for each iteration (dev) |

The output is equal in the two tiers for each shape and each count.
A handle that is dropped with no await is rejected (S013), so that
shape has no row.

Cause of the two rows in bold. The object W holds the frame F through
a counted field, and the count of F is 1. F completes, and its
completion value holds the address of W. The collector takes each
registered frame as a root, and each completion value as a root
(`runtime/src/context/memory.rs`, the root sets `async_frames` and
`completion`). So W is marked, the release of unreachable counted
fields (`runtime/src/context/counted.rs`) does not release the field
of W, and the count of F stays 1. The locals of a completed frame are
not the holder: the same shape with an `i32` result is reclaimed.

Judgment. The two rows in bold are typical: a loader that returns its
receiver or its container. The row of the two methods that await each
other is a wait that never ends; the tasks are unfinished work
(§70.3.6), and the row is contrived.

Not measured: a task that waits on a host completion source and whose
object is dropped (the harness did not link the native fixture). The
ship run of the second row in bold at N = 100,000 took several
minutes; the cause is not measured.


## G8 — polls on the return path of a recursion

Measured 2026-11-22 at `9a8fa86a`, in a copy of the tree; the tree did
not change. Origin: a review of the placement by another agent.

The rule that the placement contradicts: §342 rule 11d and §345 rule
3 state that each path reaches a poll after a count of allocating
instructions that only the module bounds. The argument counted the
depth of the call tree. It did not count the chain of returns: after
the deepest call returns, each frame that waits runs its continuation,
and each continuation can allocate before the frame returns.

Fixture: `build(depth)` calls `build(depth - 1)` and then returns a
new object. Automatic mode, minimum headroom 64 bytes, one explicit
collection first. Objects over the base before the outermost exit, and
automatic collections in the call:

| Depth | Objects | Collections in the call |
|---|---|---|
| 32 | 33 | 0 |
| 256 | 257 | 0 |
| 1,024 | 1,025 | 0 |

The figures are equal in the ship tier and the dev JIT. A control
whose continuation does not allocate keeps 1 object. A tree-shaped
recursion is served by the entry poll of each second call.

Prototype placements, objects before the exit at depths 256 and 1,024:

| Fixture | Present | Direct cycle | Plus each indirect call | Plus each handler | Cycle of the graph with indirect edges, and its handlers |
|---|---|---|---|---|---|
| The fixture; two functions that call each other | 257, 1,025 | 2, 5 | 2, 5 | 2, 5 | 2, 5 |
| A recursion through a function value | 257, 1,025 | 257, 1,025 | 2, 5 | 2, 5 | 2, 5 |
| A handler that allocates, with a throw from the bottom | 259, 1,027 | 259, 1,027 | 259, 1,027 | 24, 15 | 24, 15 |
| `f` calls `g` through a function value, `g` calls `f` directly, and only the continuation in `g` allocates | 129, 513 | 129, 513 | 129, 513 | 129, 513 | 10, 3 |

The last column is the placement that closes each row: a call graph
with an edge from each indirect call and each callback operation to
each function whose address is taken; a poll after a call into the
cycle of the caller if the continuation can allocate before a poll;
a poll in each handler of a function of a cycle. A direct call to a
function with an entry poll counts as a poll in the scan of the
continuation.

Cost of that placement:

| Item | Result |
|---|---|
| Poll sites, 497 accept entries | 1,535 to 1,564 |
| Poll sites with a poll after each indirect call and in each handler | 2,764 |
| Emitted C of `tree`, `callbacks`, `sort`, `particles`, `queen`, `fib-recursive`, `collect` | identical to the present placement, with the fact correction of the next row |
| `Coerce` of a class to its nullable type has the fact "can allocate" | with that fact `tree` gets 2 more polls for each inner node; the operation does not allocate |
| A recursion of depth 1,000, 100,000 calls, one more poll for each frame | ship tier 1,974 and 1,978 ms to 2,025 and 1,993 ms; dev JIT 7,027 and 6,914 ms to 6,942 and 6,948 ms |
| `tree` with one more poll for each node (3.9 million) | ship tier 103.2 and 104.8 ms to 102.7 and 102.5 ms |
| Stress sweep with each placement | dev JIT 611 of 611, ship tier 609 of 609, interpreter 499 of 499 |
| LIR snapshot | 17 lines added |

A result of a call that the continuation reads is rooted at the new
poll in the three tiers: the root storage follows the liveness of the
finished LIR, not the poll positions.

Cases that the placement does not cover: a recursion through the host
(a foreign call, then an export call); a value that passes up through
nested generators.

§351 lands the placement of the last column. Its Phase Review found
one more chain, the resumes of an async recursion in one drain, and
§351 rule 12 gives the drain a poll between two jobs. The evidence is
in `compiler.md` §351.5.
