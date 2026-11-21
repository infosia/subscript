# Automatic collection — trial on the branch `gc-auto`

Status: **MEASUREMENT (G0).** No contract changed. No production code
changed. The prototype is on the branch `gc-auto-proto` and does not
land.

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

## Rounds

| Round | Content |
|---|---|
| G0 | Measurement. Lands nothing. |
| G1 | Contract: invariant 2, Q6, Q7, lifetime, failure delivery. Red corpus entries. |
| G2 | Threshold, pending flag, host-boundary service, host policy API. |
| G3 | LIR poll, three lowerings, stress in the gate. |
| G4 | Removal of `Context.free` (owner decision 3). |
| G5 | Phase Review. |

Section numbers are assigned at the merge.

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
