# §92 — an async call starts its body at the call

Status: **landed** at `30318d5`. Contract:
`specs/blocks/compiler.md` §92 (`dc119e7`, amended `b1eaa56`,
`1b271c4`), `collisions.md` C8 and C16. Origin: an async proposal of
2026-09-08 asked for concurrent progress; the measurement that
answered it found a divergence nobody had decided.

## What was wrong

An async call created a handle and ran nothing. `node` runs an async
body synchronously to its first `await`. Measured at `34e0af3`:

| Program | `node` | subscript |
|---|---|---|
| two handles, body awaits once | `start1 start2 end1 end2` | `start1 end1 start2 end2` |
| a call that never awaits | `quick1 after-create` | `after-create quick1` |

C8 described the async model in detail and said nothing about start
timing. No corpus entry could see it: every async accept entry
declared `js-comparable: no`, because each used `Context.suspend()`.

## What landed

An async call runs the callee to its first suspension, or to its
return, before the caller continues (§92.1 rule 1). The form states
it: `AsyncHandleCreate`'s definition changed from "create an async
coroutine frame without polling it" to the four facts of rule 1b.

The reach is stated, not assumed. Rule 1 matches `node` where a start
is observable. It does not reach JavaScript's interleaving, which
comes from the microtask queue: `node` yields once at every `await`,
even a settled one (`f(1); f(2)` with `await 0` prints `a1 a2 b1 b2`).
That needs a queue, which C8 and Q34 exclude. Recorded as C16, with
two programs.

## Rounds

1. **Stopped at the form** (core principle 8, correctly). The
   handoff named files but not the form change; `AsyncHandleCreate`'s
   own definition said "without polling it", so a consumer that
   polled would contradict the form. Rule 1b answers it.
2. Implemented. a184 (`js-comparable: yes`) is the first async entry
   the node comparison runs. a185 holds the two C16 shapes with the
   measured `node` order in its header.
3. Coverage and record, after a review: the array-invalidation fact,
   t54's Red at the pin, a185's second shape, a distinguishing
   root-queue test, the collision-id scanner, the divergence variant,
   the tutorial.
4. Two more from a second review: a prose count `t54` falsified, and
   the last corpus-entry count pin in the tree (`cemit.rs`, a site
   §88 missed — recorded at `d413529`).
5. **A revert.** Round 3's array test drove a pruning pass into the
   lowering: `AsyncHandleCreate.invalidates` filtered to arrays with
   a later use. A third review removed it. No golden and no
   `.expected` moved with it, root storage never reads the field, and
   it gave one field two meanings — the committed golden showed a
   pruned list and an unpruned one in one function, which is the
   shape §86.1 rule 2a records. It also rewrote
   `root_storage::plan`'s walk, minus rule 2b's mention set, and
   added two panics to library code. The test's negative half is now
   a total assertion derived from the array definitions in the
   instruction stream, not from the field it checks.

**The lesson, once:** a test's negative half is not a reason to
change production behaviour. Core principle 13 caught it — the pass
had no stated problem outside the test written in the same round.

## The goldens that moved (§2)

`corpus/accept/a154-held-async-handle.expected`: both bodies now
start at their calls, so `work1:start=0` precedes `main:between` and
`work2:start=1` precedes `main:held`. `work1:resume` moves from 1 to
3 and `main:first` from 11 to 13, because `work(2)` runs
`progress += 2` before `first` is awaited.

`corpus/accept/a155-async-handle-array.expected`: `start=1 start=2
start=3` precede `held=3`.

`codegen/tests/lir-goldens/corpus.txt`: the LIR text of the async
entries.

## Also recorded

§70.3 rule 6 said "a frame cannot hold a handle to itself today" and
asked for the shape to be recorded if it appeared. It appeared: a
handle stored into a module global and awaited from inside its own
body. The checker accepts it, and both tiers end abnormally. **The
control**: `function f(n: i32): i32 { return f(n + 1); }` gives the
identical dev-tier line and exit code, so this is unbounded
recursion, not a defect of the handle. The language traps no stack
depth in either shape.

§70.1 rule 2's reason narrowed: an abandoned handle now runs the
prefix before its first suspension.

## Open

Concurrent completion (§92.1 rule 7). `a94` shows the pump
interleaving two pending roots; a handle from a call is not in that
root set. The next request states its own problem.

## Gate

```text
gate full 4813bfb75051d45ed5f3d8e5967e3a0c3d531863 dirty:23 debug 1303/0/2 release 1301/0/2 skips 2/0 clippy 7/18/13 goldens-moved 3 exit 0
```

A run before this one failed `term_during_debug_deletes_the_partial_record`.
That case compared the whole `target/gate/` directory against a
snapshot, and it runs inside a real gate, so a concurrent writer made
it fail on a record it did not own. §85.3 case (i) now asks for the
run's own record; fixed at `85decf6`.
