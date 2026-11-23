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

A second review at `524edac8` found one more return path: a function
that resumes a generator and then allocates. §352 states for each LIR
instruction what script code it runs, builds the graph from that
fact, and checks the fact in the interpreter. The evidence is in
`compiler.md` §352.5.

## G9 — the time of one collection against the live heap

Measured 2026-11-22 with the record of §353 (`last_ns`,
`last_words_read`), in the ship arena of the runtime, release build,
arm64 macOS. Each row is the median of 5 explicit collections, second
of two runs. An object has 32 bytes.

| Shape | Live MiB | Words read | Time, ms | ns for each word | ms for each MiB |
|---|---|---|---|---|---|
| An array of references to objects | 1 | 131,075 | 1.698 | 12.96 | 1.698 |
| | 4 | 524,293 | 9.160 | 17.47 | 2.290 |
| | 16 | 2,097,158 | 41.49 | 19.79 | 2.593 |
| | 64 | 8,388,611 | 189.3 | 22.57 | 2.958 |
| A chain of objects | 1 | 131,071 | 0.972 | 7.42 | 0.972 |
| | 4 | 524,287 | 4.491 | 8.57 | 1.123 |
| | 16 | 2,097,151 | 22.77 | 10.86 | 1.423 |
| | 64 | 8,388,607 | 110.7 | 13.20 | 1.730 |
| An array of numbers | 1 to 64 | 7 | 0.0004 | | about 0 |

- The time is not a function of the live bytes: a buffer that holds
  no handle costs nothing (§344). At equal words the cost depends on
  the shape: at 1 MiB the chain and the array of references read
  about 131,000 words each, in 0.97 ms and 1.70 ms.
- The cost of one word grows with the heap: 7 to 13 ns in the chain,
  13 to 23 ns in the array of references. The cause is not measured;
  the lookup of a candidate address and the cache are the candidates.
- A frame of 16.7 ms holds one collection of about 6 MiB of small
  objects that an array holds, or about 12 MiB of a chain.
- The 1 MiB row of the array of references gave 4.6 ms in the first
  run and 1.7 ms in the second.

The measurement is an ignored test of the runtime
(`the_collection_time_curve_of_the_ship_arena`), about 2 s.

After §355, with no test counter (`runtime/tests/mark_lookup_cost.rs`),
ms for one collection and ns for each word:

| Shape | Live MiB | At `b8e593c1` | After §355 |
|---|---|---|---|
| An array of references to objects | 1 | 1.611 (12.29) | 0.528 (4.03) |
| | 4 | 7.451 (14.21) | 2.608 (4.97) |
| | 16 | 40.61 (19.36) | 8.471 (4.04) |
| | 64 | 185.0 (22.06) | 31.25 (3.72) |
| A chain of objects | 1 | 0.758 (5.79) | 0.423 (3.23) |
| | 4 | 4.129 (7.87) | 1.651 (3.15) |
| | 16 | 21.54 (10.27) | 7.403 (3.53) |
| | 64 | 104.6 (12.47) | 27.39 (3.27) |

A frame of 16.7 ms holds one collection of about 30 MiB of small
objects after §355.

## G10 — the sweep against the slots below the bump marks

Measured 2026-11-23 at `06e4a12d`, release build, arm64 macOS, the
ship arena. Origin: a downstream report of collections of 0.9 to
1.1 ms with 1.5 MiB of live bytes, 231,736 to 447,430 slots below
the bump marks, and about 14,600 words read. The round built three
prototypes and reverted each change.

The rule that a landed prototype works against: each one adds work
to the allocation path, which `perf-gate` guards.

Form of the measurement: 23,040 live blocks (1.5 MiB) in four size
classes, H slots below the bump marks, 200 explicit collections, a
64 MiB write before each collection to empty the cache. "Dense": the
live blocks fill 24 chunks and each other chunk is free. "Sparse":
each chunk holds live blocks. "Full": each slot is live. g is the
part of the free slots that the program allocates again before each
collection.

State at the pin, median µs.

| H | Shape | g | Pause | Before the sweep | Sweep of the size classes | ns for each slot |
|---|---|---|---|---|---|---|
| 100k | dense | 0 | 402 | 239 | 160 | 1.60 |
| 450k | dense | 0 | 1,054 | 289 | 762 | 1.69 |
| 450k | dense | 1.0 | 964 | 292 | 671 | 1.49 |
| 450k | sparse | 0 | 1,153 | 474 | 679 | 1.51 |
| 450k | sparse | 1.0 | 1,187 | 575 | 594 | 1.32 |
| 1M | dense | 0 | 1,765 | 322 | 1,441 | 1.44 |
| 4M | dense | 0 | 5,818 | 378 | 5,436 | 1.36 |
| 4M | sparse | 0 | 6,486 | 1,035 | 5,444 | 1.36 |

- The sweep reads the state word of each slot below a bump mark:
  1.0 to 2.0 ns for each slot. The state of the slot does not change
  the cost. The sweep of the large allocations is below 1 µs.
- The time before the sweep is the push of the roots (about 60 µs for
  23,040 root words) and the mark. It does not follow H.
- The cost of one marked word grows with the count of chunks: 7.8,
  9.8, 11.5, and 13.8 ns at 108, 472, 1,044, and 4,168 chunks in the
  dense shape; 12.6 to 42 ns in the sparse shape. The candidates are
  one more step of the search of the chunk map for each doubling, and
  the cache for live blocks in many chunks. Each is not measured
  alone. This is the growth that G9 records.
- The figures of G9 come from a unit test of the runtime, whose build
  counts each lookup in an atomic counter. The effect on G9 is not
  measured.

The prototypes. B: a count of occupied blocks for each chunk; the
sweep does not read a chunk with the count 0. C: one occupancy bit
for each slot. D: one flag for each chunk, set at an allocation into
the chunk and at the end of a sweep that found a live block.

Sweep of the size classes, median µs.

| H, shape, g | Pin | B | C | D |
|---|---|---|---|---|
| 450k dense 0 | 762 | 20 | 26 | 18 |
| 450k dense 0.1 | 735 | 70 | 82 | 58 |
| 450k dense 1.0 | 671 | 757 | 821 | 661 |
| 450k sparse 0 | 679 | 701 | 27 | 669 |
| 450k sparse 1.0 | 594 | 602 | 791 | 581 |
| 4M dense 0 | 5,436 | 24 | 62 | 24 |
| 4M sparse 0 | 5,444 | 5,413 | 88 | 5,372 |
| 4M dense 1.0 | 7,286 | 7,300 | 8,507 | 7,205 |

| | ns for one bump allocation | ns for one reuse from a free list |
|---|---|---|
| Pin | 5.92 to 6.08 | 6.26 to 6.30 |
| B | 6.29 to 6.40 | 6.32 |
| C | 6.87 to 6.95 | 6.80 to 6.81 |
| D | 6.06 to 6.09 | 6.28 to 6.31 |

- `perf-gate --gate` met each criterion with each prototype; the
  `collect` ratio of the ship tier was 1.13x to 1.17x, inside the
  spread of four runs of the pin (1.15x to 1.18x).
- The tests of the runtime passed with each prototype, each with a
  check of its record on each sweep, and a control that removes one
  update of the record fails. The stress sweep passed with B and C.
  With D it passed in 3 of 4 runs; the text of the one failure was
  not kept, and the pin was not run for a comparison.
- No prototype reduces a sweep in which each slot was allocated
  after the last collection (g = 1.0). C is 15 to 25 % slower there.

Decision: no prototype lands.

- The allocator takes a block from a free list before it moves a
  bump mark. So a bump mark moves only when the free list of its
  class is empty, and the slots of that class then hold live blocks
  and blocks that the program allocated after the last collection.
  In the downstream run the slots below the bump marks grew at each
  collection (231,736, 386,585, 447,430). That is the g = 1.0 form:
  the sweep reads blocks that it must retire, and blocks that were
  free before are a small part.
- The count of those blocks follows the headroom. 16 MiB of requests
  of about 50 bytes are about 320,000 blocks, the size of the
  downstream figures. A smaller headroom gives more collections, each
  with a shorter sweep. The time before the sweep (about 0.3 ms in
  the downstream run) does not change.
- The record of §353 does not show the split. A census of the slot
  states at the entry of a sweep in the downstream program decides
  whether D has a use there.

Open.

1. The cost of one marked word against the count of chunks, and its
   two candidate causes.
2. A sweep that runs in parts at later allocations takes the retire
   work out of the pause. It changes when the live bytes fall, and no
   design exists.
3. The one failed stress run with prototype D.

## G11 — the cost of one marked word

Measured 2026-11-23 at `b8e593c1`, release build, arm64 macOS, the
ship arena, the heaps of G10. The figures are medians of 31 passes
in ns for each word, with a 64 MiB write before each pass. The round
reverted each change. It closes open items 1 and 3 of G10.

The work of one candidate word at the pin:

1. A search of the sorted chunk map (`partition_point` over pairs of
   16 bytes): ceil(log2 n) dependent reads, 7, 9, 11, and 13 at 108,
   472, 1,044, and 4,168 chunks.
2. A read of the chunk record, then of the block state.
3. For a word that is no block of a chunk: a lookup in the map of the
   large allocations, a hash map with the default hasher. An empty
   map returns before it hashes. A map with one record or more costs
   about 7 ns for each such word.

| Chunks | Shape | Mark | Lookup only | State read only | A word that is no block, large map empty | The same with one large record |
|---|---|---|---|---|---|---|
| 108 | dense | 7.3 | 5.4 | 1.3 to 1.5 | 4.0 | 10.9 to 11.7 |
| 472 | dense | 9.2 | 7.2 | 1.2 | 5.7 | 12.5 to 13.2 |
| 1,044 | dense | 11.2 | 9.2 | 1.3 | 7.5 | 14.6 |
| 4,168 | dense | 13.4 | 11.4 | 1.2 | 9.7 | 17.4 |
| 108 | sparse | 11.0 | 6.5 | 2.9 to 3.9 | 4.0 | 11.0 |
| 4,168 | sparse | 41.2 to 41.6 | 15.0 | 5.1 to 5.6 | 9.7 | 17.4 |

- The lookup has the same time with a full cache and with an empty
  one. Its cost is the chain of dependent reads of the search.
- In the dense shape the growth of the mark (6.1 ns) is the growth
  of the lookup (6.0 ns).
- In the sparse shape the search gives 8.6 ns of the growth and the
  state read 2 to 5 ns. The mark loop has about 21 ns more at 4,168
  chunks than a loop of lookup and state read with no work list.
- Each word that is no block has the same cost: 0, a small integer,
  a pointer outside the arena, the bits of a float.
- A chunk comes from `alloc_zeroed` with an alignment of 16. Most
  bases had an alignment of 64 KiB (4,126 of 4,168; 53 of 108), which
  the allocator does not promise.

Prototypes, at 108 and 4,168 chunks.

| Variant | Lookup | Mark, dense | Mark, sparse | A word that is no block, one large record, at 4,168 | Chunk creation, ns |
|---|---|---|---|---|---|
| Pin | 5.4 and 11.4 | 7.3 and 13.4 | 11 and 41 | 17.4 | 1,040 to 1,180 |
| A table from the 64 KiB page of an address to a chunk, chunks not aligned | 2.1 and 2.1 | | | | |
| The address hasher on the large map | as the pin | 6.8 and 13.0 | 10.4 and 38.3 | 10.0 | 1,179 |
| The page table and the address hasher | 2.1 and 2.1 | 3.3 and 4.4 | 7.0 and 20.9 | 1.7 | 1,067 |
| Those and a prefetch of the word 8 places ahead | 2.1 | 3.5 and 4.4 | 5.4 and 13.1 | 1.8 | 1,024 |
| Chunks with an alignment of 64 KiB, a hash of the page, the address hasher | 1.6 and 1.8 | 3.2 and 4.2 | 7.4 and 20.1 | 1.4 | 836 |
| A search over the keys only, with no branch | 4.1 and 8.2 | 6.1 and 10.4 | 10.2 and 35.6 | 13.7 | 1,150 |
| A cache of the last chunk | 1.3 and 1.4 (6.1 sparse) | 3.7 and 4.8 | 7.6 and 26.9 | as the pin | 1,194 |
| A range test before the search | 6.1 and 12.1 | 8.5 and 14.6 | 12.7 and 45.6 | 7.6; 18.9 inside the range | 1,107 |

The page table and the address hasher, against the pin:

| Item | Pin | With the two changes |
|---|---|---|
| `perf-gate --gate`, `collect`, ship tier, two runs | 36.689 and 36.741 ms (1.16x and 1.17x) | 30.291 and 30.415 ms (0.97x and 0.97x) |
| The same, `a22` | 5.275 and 5.268 ms | 5.263 and 5.258 ms |
| A bump allocation; a reuse from a free list | 5.5 to 5.7 ns; 6.0 to 6.2 ns | 5.68 ns; 6.13 ns |
| Tests of the runtime, release; stress sweep | | pass; pass |

The shapes of G9, ms for one collection (ns for each word). The unit
test of G9 counts each lookup in an atomic counter of a test build,
which adds 1 to 3.5 ns for each word; the second column has no
counter.

| Shape | Live MiB | Pin | With the two changes |
|---|---|---|---|
| An array of references to objects | 1 | 1.54 (11.7) | 0.506 (3.86) |
| | 4 | 7.28 (13.9) | 2.574 (4.91) |
| | 16 | 39.4 (18.8) | 8.679 (4.14) |
| | 64 | 181 (21.6) | 34.12 (4.07) |
| A chain of objects | 1 | 0.756 (5.8) | 0.457 (3.49) |
| | 4 | 4.01 (7.7) | 1.772 (3.38) |
| | 16 | 20.9 (10.0) | 8.965 (4.28) |
| | 64 | 102.8 (12.2) | 29.32 (3.50) |

An object of 32 bytes has six payload words, and five are 0. So the
array shape reads mostly words that are no block, with a large map
that holds the array: the default hasher was the larger part there.

Decision: the page table and the address hasher land, as
`compiler.md` §355. The aligned form is not selected: its gain over
the page table is small, and the cost of an aligned chunk on Linux
and Windows is not measured. The prefetch is not selected: it needs
code for each architecture, and it was measured on arm64 only.

§355 lands the two changes; its record has the figures of the
landed form. A run of the stress sweep alone after an edit of the
runtime can fail at its first ship link (§355.5 item 3); that is a
candidate cause of the failed run of G10.

The failed stress run of G10 with prototype D: the pin passed 6 of 6
runs and prototype D, with its check, passed 6 of 6. Each run was
alone on the host, after a deletion of the runtime archive. The
cause of the one failure is not found; in that round the sweeps, a
control, and the benchmark runs were minutes apart, and the round
changed patches between runs.

Open.

1. The sparse shape at many chunks stays at 7 to 21 ns for each word:
   the wait for the state word of a block in a chunk that the cache
   does not hold. The prefetch reduces it to 5.4 to 13.1 ns.
2. Why the mark loop overlaps those waits less than a loop with no
   work list.

## G12 — a sweep that runs at later allocations

Measured 2026-11-23 at `d7ecb78a` (after §355), release build, arm64
macOS, the ship arena. The round built one prototype and reverted
each change. It answers open item 2 of G10.

The rule that a landed form works against: none, if no observable
result changes. The form adds work to the allocation path, which
`perf-gate` guards, and it changes each reader of a block state.

The prototype.

- A collection marks, sweeps the large allocations, and reads no
  chunk. It sets each chunk to "pending".
- The mark word holds an epoch. A block is live if its state is the
  live word of its chunk or the mark word of the Context. So a mark
  of a former collection is not a mark of the next one, and no
  collection sweeps a chunk.
- Each chunk counts its marked blocks and its free blocks. The live
  bytes after a collection come from the counts of the mark.
- The arena sweeps a pending chunk before the first allocation into
  it, and sweeps pending chunks of a class when the free list of the
  class is empty. A pending chunk that holds only free blocks is
  read at no time.

The heaps of G10, median µs. "Cycle" is one pause and the
allocations up to the next collection.

| H, shape, g | Pause, pin | Pause, prototype | Cycle, pin | Cycle, prototype | ns for one allocation, pin | The same, prototype |
|---|---|---|---|---|---|---|
| 450k dense 0 | 851 to 873 | 151 | 851 to 873 | 151 | | |
| 450k dense 0.1 | 814 to 844 | 159 to 161 | 1,083 to 1,105 | 482 to 486 | 6.11 to 6.26 | 7.61 |
| 450k dense 1.0 | 786 to 818 | 181 | 3,546 | 3,644 to 3,651 | 6.38 to 6.45 | 8.12 |
| 450k sparse 1.0 | 931 to 973 | 437 to 440 | 3,651 to 3,696 | 3,890 to 3,895 | 6.27 to 6.47 | 8.08 |
| 450k full 0 | 3,151 to 3,220 | 3,111 to 3,118 | | | | |
| 4M dense 1.0 | 7,344 to 7,376 | 196 | 32,632 to 32,956 | 33,312 | 6.35 to 6.44 | 8.33 |
| 4M full 0 | 36,818 | 31,067 to 31,168 | | | | |

A cycle of the downstream form: 1.5 MiB live, 16 MiB of headroom,
the automatic trigger, frames of 2,000 allocations of about 50
bytes, about 175 frames for each collection, 20 collections.

| Item | Pin | Prototype |
|---|---|---|
| Pause, median µs | 524 to 530 | 195 to 199 |
| Pause, max µs | 797 to 828 | 235 to 267 |
| The allocations of one frame, median µs | 12.7 to 13.2 | 15.6 to 15.7 |
| The same, p99 and max | 18.2 and 20.8 to 43.3 | 20.4 to 22.5 and 40.6 to 45.3 |
| One sweep of a chunk, median, p99, and max µs | | 1.2, 2.0 to 2.3, 11 to 25 |
| Work of one cycle | about 2.8 ms | about 170 µs more |

| Item | Pin | Prototype |
|---|---|---|
| A bump allocation with no chunk pending, ns | 6.03 to 6.05 | 6.26 to 6.28 |
| A reuse from a free list with no chunk pending, ns | 6.27 to 6.53 | 6.61 to 6.63 |
| The mark | | about 1 ns more for each marked block; 5 to 17 % by shape |
| `perf-gate --gate`, `collect`, ship tier, ms | 29.722 and 29.391 | 30.714 and 30.749; each criterion met |
| The same, `a22` | 5.125 and 5.253 | 5.278 and 5.278 |
| Tests of the runtime | | two tests fail, each on a fact that the form changes |
| Stress sweep, two runs | | pass |

- The sweep leaves the pause. The pause is the roots and the mark.
- The work of a cycle grows by 1 to 6 % where each slot was
  allocated after the last collection, and falls by 40 to 80 % where
  most free blocks stay free.
- The two tests: one reads the state word of a live block after a
  collection, which is a mark word until the sweep of its chunk. One
  is the control of the check that compares the live-byte counter
  with a walk at a collection; the prototype sets the counter at each
  collection, so the check cannot fail. A landed form needs that
  check before the mark.
- A temporary check compared the counter with a walk at each lazy
  sweep and at each collection; two controls that break the form
  fail it.
- The form changes: the time of §353 does not hold the sweep of the
  chunks; the order of a free list; the state word of a live block in
  a pending chunk. The live bytes, the trigger, the words read, the
  reserved bytes, and the output of a program do not change.

Decision: not landed (owner, 2026-11-23). The downstream program
runs on the form of §355 first.

- The gain is the sweep of one cycle of garbage, which follows the
  headroom: about 0.33 ms for each collection at 16 MiB. A host can
  reduce it with the headroom. At a large live heap the mark is the
  larger part of the pause, and this form does not reduce it.
- The cost is on each allocation (about 4 % of a bump allocation and
  of the `collect` workload), on the mark, and in the form: each
  reader of a block state must use one test, the mark word wraps
  after 2^32 collections, and the drift check moves.

A landed form needs: one accessor for the live test and a check that
no other reader exists; the drift check before the mark, with a
control; a test of the epoch wrap; tests of a word that points to
garbage in a pending chunk, of `Context.free` in a pending chunk, of
a pending chunk over many collections, and of a chunk that is read
at no time.
