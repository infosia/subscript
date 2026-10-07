<!-- §176 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 176. A dropped generator releases its frame

*(Added 2026-10-07.)* Origin: `compiler.md` §171.3 items 1 and 4. On
2026-10-07 the owner selected both items. The measurement round at
`e8cf6e09` is `specs/tracking/s176-count-measurement.md`.

Problem: a generator that is suspended and then dropped keeps its frame
and every count that the frame holds (§171 rule 13). The measurement
found the effect in good-faith programs, in the three tiers:

| Shape | Output J/C/I | Retained tasks J/C/I | `node` |
|---|---|---|---|
| `break` out of a for-of after the first yield | `v 1`, `end` | 1/1/1 | `v 1`, `end` |
| `return` out of the consumer | same | 1/1/1 | same |
| a local iterator after one `next()` | same | 1/1/1 | same |
| a generator object that is never started | `unused`, `end` | 1/1/1 | same |
| each shape above, with a failed task that no program observes | `v 1`, `end`, exit 0 | 1/1/1 | unhandled rejection, exit 1 |

A kept count keeps a failed task live, so its unobserved exception never
traps (§116.1 rule 4). `Context.collect()` after the drop does not
release the count.

Item 4 does not reproduce. A suspended `await` of a borrowed element
holds its own count on the awaited task:
`runtime/src/context/async_scheduler.rs` `Context::async_await` calls
`async_retain` before it registers the continuation, and the
interpreter's `register_continuation` increments the owners. Twelve
variants (a reset global, a static field, a freed field, a nested
array, a loop, Map values) print the `node` output with zero retained
tasks in the three tiers.

JavaScript calls the iterator's `return()` at a for-of `break`, which
runs only `finally` blocks. This language rejects `finally` (C6), so
no generator body code runs at a drop in either language.

### 176.1 Rules

1. **A generator is a counted type.** §171 rule 1 adds
   `Generator<Y>`, for every `Y`. Each rule of §171 and §172 that names
   a counted type applies to it: a local copy, an array element, a
   class field, a `Map` value, a for-of subject, a temporary, and a
   collection sweep.
2. **The generator has a holder count.** A generator call returns a
   fresh owner with a count of one. A copy acquires, and a holder exit
   releases, on the one lowering path of §70 rule 2b.
3. **The last release closes the frame.** When the count reaches zero,
   the runtime releases each counted value that the frame owns at its
   current state, then frees the frame and removes it from each runtime
   registry. No generator body code runs. The owned values of a frame
   that is not started are its parameters. The owned values at a
   suspension are the counted values that are live across that
   suspension: parameters, lexical locals, hidden for-of subjects, and
   held temporaries. An exhausted generator owns no value.
4. **A cleanup description per state.** The LIR carries, for each
   suspension and for the start state, the description of the counted
   values that the frame owns there, with the number of counts of each
   value. The builder writes it from its lexical binding table. The
   value liveness cannot derive it: two holders of one value own two
   counts and share one value id (`specs/tracking/s176-count-measurement.md`,
   "Implementation: the LIR cleanup input").
4a. **Count routes on edges.** Each argument of a block edge, and of a
   suspension successor, carries an ownership flag: *owned* moves one
   count from the argument value to the block parameter, and *borrowed*
   moves none. The builder writes it from its binding table. A block
   parameter is owned on every incoming edge or borrowed on every
   incoming edge. A balance keyed by value origin cannot replace the
   flag: a `let` holder that takes another value on one path gives two
   origin balances at one join, with the same holders on both paths
   (`specs/tracking/s176-count-measurement.md`, "Ordinary joins need
   count routes").
4b. **The count balance.** The verifier derives, separately, the number
   of counts that the frame owns for each value at each point. It is a
   forward dataflow over the count action of each instruction (§171
   rule 9) and the flags of rule 4a: a fresh owner, a parameter that the
   frame owns, an acquire, and an owned incoming edge add one; a
   release, a transfer, a store that consumes, and an owned outgoing
   argument subtract one. The balance counts frame owners only. A
   release of a borrowed value (a load with no fresh owner) that the
   frame owns no count of is the release of a container's displaced
   owner: §171 rule 9 pairs it with its replacing store, and it does not
   change the balance. The verifier reports a balance below zero, and
   a block parameter that is owned on one incoming edge and borrowed on
   another. At each suspension and at the start state, it compares the
   balance with the description (core principle 9). Unit tests build a
   description with a missing count, a description with an extra count,
   an owned argument with no count, and a parameter with mixed flags,
   and read each message.
5. **The release can trap.** A last release that frees a task frame
   with an unobserved exception traps with §116.1 rule 4: the report
   text and the position of the last `throw`, as a §171 element release
   reports it (`t87`). The releasing instruction carries its trap site
   (§171 rule 10). A lexical exit needs no source position of its own.
6. **Native layout.** The native generator frame carries a holder
   count. The tracking note states its offset and the offsets that
   move. A generator that a reload keeps keeps its count.
7. The three tiers give the same output, the same counts, and the same
   trap.
8. §171 rule 13 is replaced by this section.
9. **Surface consequences of rule 1.** Two existing rejections now
   cover generators, because each follows the counted type:
   - §175: a lambda that captures a generator binding stays in the
     block of that binding. The block exit releases the generator.
   - §171 rule 5a: a callback method that carries a generator in a
     callback position (an `Array` or `FixedArray` callback or reducer,
     `Map.groupBy`) is rejected with S014. The API admission trace adds
     15 omitted cells.
   No existing accept, warn, trap, or interop entry, example, or
   benchmark workload is rejected.

### 176.2 Acceptance

1. Red first, at the contract pin: accept entry `a346` (the shapes of
   the table above with completed tasks, a generator in a reference
   field that `Context.free` frees, a generator in an array that `pop`
   discards, and an alias that keeps the original live), with
   `js-comparable: yes` if `node` agrees; trap entries `t105` (`break`
   with a failed unobserved task) and `t106` (a local iterator with a
   failed unobserved task), each trap 29 at the last `throw` when the
   generator is released. Record the pin result of each in each tier.
2. Unit tests: a three-tier test of the retained-task count after each
   shape of `a346`, with an exhausted same-shape control; a drop at a
   yield inside an inner block (the case that a normal-return cleanup
   list misses in the measurement); the verifier tests of rule 4b.
3. Cost: each benchmark workload that uses a generator, and the
   interpreter corpus, best of three, within 1.05 of the pin. State
   the cost of each new gate test (core principle 15).
4. The LIR text golden moves for generator functions and the new
   entries. No other `.expected` golden moves.

### 176.3 Sections this one amends

- §171 rule 1: `Generator<Y>` is a counted type.
- §171 rule 13 and §171.3 item 1: replaced and closed.
- §171.3 item 4: closed by the measurement above.

### 176.4 Open

The Phase Review found these. None is open as CRITICAL or MAJOR.

1. A generator in a cycle with its container stays rooted while the
   async frame that built it runs, if a receiver of the build is held
   across an `await` (`s.queue.push(spawner(s, await x))`). The
   receiver is in frame-lifetime storage (§172 rule 7). The three tiers
   agree: no trap at a collect during the frame, and trap 29 at the
   first collect after the frame finishes. A test pins both states.
2. `coroutines.forEach(c => c.next())` is rejected by rule 9 (S014); the
   message gives the `for…of` form.
3. The start-state description has no independent check: the start
   balance and the description both follow from the parameter kinds.
   The verifier checks each yield description.
