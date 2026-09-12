<!-- §92 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 92. An async call starts its body at the call

*(Revised 2026-09-08 by §94, landed 2026-09-09.)* §94 supersedes
this section's completed-await, root-exclusion, and concurrent-progress
rules. The measurements below record the earlier implementation.

*(Owner decision 2026-09-08.)* Origin: the async proposal of
2026-09-08 asked for concurrent progress. The measurement that
answered it found a divergence nobody had decided.

An async call creates a handle and runs no part of the callee.
Nothing in this contract or in `collisions.md` C8 states that, and C8
describes the async model in detail. Measured at `34e0af3`, this
host, two shapes:

| Program | `node` | subscript |
|---|---|---|
| `a = work(1); b = work(2); await a; await b;` where `work` prints, awaits once, prints | `start1 start2 end1 end2` | `start1 end1 start2 end2` |
| `h = quick(1); print("after-create"); await h;` where `quick` prints and never awaits | `quick1 after-create` | `after-create quick1` |

JavaScript runs an async body synchronously to its first `await`.
This language defers all of it to the first `await` of the handle.
The corpus cannot see the difference: `a154` pins the deferred order
as a golden, and every async entry declares `js-comparable: no`
because it uses `Context.suspend()`. A divergence this project did
not decide reads as a decision (CLAUDE.md, "Compiler and oracle").

The owner decides to match JavaScript **at the start of a call**, and
to record the rest as a decided divergence.

*(Amended 2026-09-08, after the round measured what "match
JavaScript" reaches.)* JavaScript interleaves two async chains
through its microtask queue: `await` yields once even when its
operand is already settled. Measured with `node v24.18.0`:
`async function f(id) { console.log(\`a${id}\`); await 0;
console.log(\`b${id}\`); } f(1); f(2);` prints `a1 a2 b1 b2`. Reaching
that order needs a queue, which C8 and Q34 exclude and which the
host-owned loop has no place for. Rule 1 below therefore matches
JavaScript where a start is observable, and rule 1a states where the
two still differ.

Measured reach of rule 1, against `node`:

| Shape | With rule 1 | `node` | Same |
|---|---|---|---|
| a call that never awaits | `quick1 after-create` | `quick1 after-create` | yes |
| two handles whose bodies suspend | `start1 start2 end1 end2` | `start1 start2 end1 end2` | yes |
| two handles whose inner await is already complete | `start1 leaf end1 start2 leaf end2` | `start1 leaf start2 leaf end1 end2` | no |

### 92.1 Rule

1. **The body starts at the call.** An async call runs the callee
   from its first statement to its first suspension point, or to its
   return, before the call's value reaches the caller. The value is
   the handle, as §70 has it.
2. **A callee that does not suspend completes at the call.** Its
   handle is complete when the caller receives it. Under §94, a later
   `await` suspends and reads the stored result on queued resumption.
   §70's required-await rule is unchanged.
2a. **An abandoned handle has already run its prefix.** §70.1 rule 2
   rejects a dropped handle, and where that analysis approximates,
   the callee's statements before its first suspension have already
   run. A program that creates a handle and never awaits it is
   rejected; a program that overwrites one after creating it has run
   the overwritten callee's prefix.
3. **The caller does not suspend.** After the callee suspends or
   returns, the caller continues at the statement that follows the
   call. An `await` in the caller is the only construct that
   suspends the caller.
4. **Order is call order.** Two calls start their bodies in the
   order the calls run. A trap in a started body reports at the call,
   with the callee's position.
5. **Export kicks keep their order.** An exported async function
   starts at its kick. §94 replaces its later root-poll behavior
   with continuation registration and host checkpoints.
6. **One order in three witnesses.** The dev JIT, the ship C, and
   the reference interpreter produce one byte sequence, as the
   standing gate requires.
1a. **Every await suspends.** §94 replaces the former completed-await
   fast path. C16 retires under that contract.
1b. **The form carries the start and the driver.** `AsyncHandleCreate`
   starts the body to its first await or return. A suspension registers
   the continuation according to its kind. The handle retains its
   ownership and completion cache. §94 defines scheduler ownership.
   A trap before the first await reports before the caller continues,
   with the callee's position.
7. **Concurrent progress is decided by §94.** A called body's progress
   does not depend on whether its holder awaits it. The host checkpoint
   advances registered work with the specified FIFO order.

### 92.2 Sites

- `codegen/src/lir.rs` (`lower_async_call`, `AsyncHandleCreate`):
  the created frame runs to its first suspension at the call. The
  runtime entry exists (`subscript_rt_async_kick`), and the export
  runner uses it.
- `codegen/src/lower/func.rs`, `codegen/src/cemit.rs`,
  `codegen/src/interpreter.rs`: whatever the LIR change requires; the
  three consumers read one form.
- `specs/blocks/collisions.md` C8: the start timing, stated
  (orchestrator).

### 92.3 Corpus and gate (pre-registered exit criteria)

1. **Red at `dc119e7`, measured.** `corpus/accept/a184-async-start-order.ts`
   carries `js-comparable: yes` and holds only the two shapes rule 1
   reaches: a call that never awaits, and a chain whose body
   suspends. At the pin its output differs from `node`'s, and
   `compiler/tests/js_corpus.rs` reports it. *(This entry is the one
   the corpus lacked: every async entry before it opted out of the
   node comparison.)*
1a. **The former divergence has its own entry.** a185 records the
   two settled-await shapes. Under §94 it carries `js-comparable: yes`
   and matches the specified FIFO order. Its former C16 explanation
   and `node-order` header lines retire.
2. **Green.** The entry matches `node` byte for byte, on the dev
   JIT, the ship tier, and the interpreter.
3. **The goldens that move.** `a154`'s order changes by rule 1. Every
   moved golden is listed in the tracking note with its old and new
   bytes, under the §2 golden-change procedure. A golden that does
   not move is not touched.
4. **A trap in a started body** reports at the call with the
   callee's position: one `corpus/trap/` entry.
5. `tools/gate.sh full` green in both profiles; clippy at the
   baseline; the perf gate within its §3 thresholds.
