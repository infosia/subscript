<!-- §172 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 172. A reference object releases its counted values

*(Added 2026-10-07.)* Origin: §170.3 item 5. On 2026-10-06 the owner
divided the count fix into §171 (arrays and completions) and this
section (a class field and a `Map` value).

Problem: the measurement round at `9647c72a`
(`specs/tracking/s172-reference-holders.md`) found these holders of a
counted value (§171 rule 1) that keep a count after the value leaves
them, in the three tiers:

- A `Map` value keeps its count after `Map.set` over its key, after
  `Map.delete`, after `Map.clear`, and after `Context.free` of the Map.
  `Map.set` of a distinct task over a key keeps both tasks (2/2/2).
- A class field (an instance field, a generic field, a descriptor
  field) keeps its count when the object becomes unreachable and an
  explicit collection reclaims it. A field store that replaces the
  value and `Context.free` of the object already release it.
- The native collector retires an unreachable class or Map allocation
  with no release of its counted contents. The interpreter roots each
  script allocation permanently, so its collection reclaims no script
  object (1,000 live allocations before and after a collection; the
  dev JIT has 0 after).

A kept count keeps a failed task live, so its unobserved exception never
traps (§116.1 rule 4). Invariant 2: an explicit collection is the
release point of an unreachable reference object, so the collection
releases what the object holds.

The round measured a second form: reject a counted type in a class
field and a `Map` value. It rejects `a161`, `a162`, `a260`, `t66`, and
`t83`, and leaves an indirect holder (a `Box` in an array or a `Set`)
to the same rule. This section uses the release form.

### 172.1 Rules

1. **A `Map` owns its counted values.** For a `Map<K, V>` with a
   counted `V` (§171 rule 1), the Map owns one count of each value.
   `Map.set` acquires the stored value (a fresh owner transfers its
   count) and, over an existing key, releases the replaced value after
   the acquire. `Map.delete` releases the removed value. `Map.clear`
   releases each value. A read (`getOr`, a `values()` or `entries()`
   iteration) borrows the value; a holder that stores the read
   acquires (§70 rule 2). Growth and rehash move the values and change
   no count. A counted `K` stays rejected (S014).
   `Map.forEach` with a counted `V` is a collection function that takes
   a callback, so §171 rule 5a rejects it (S014). Measured at
   `9647c72a`: the checker accepts `m.forEach` that stores a handle
   value in a global array, and the LIR verifier then rejects the
   counted store, so no tier runs it.
2. **The release of a reference object.** `Context.free` of a class
   object or a Map, and the sweep of an explicit collection that
   reclaims one, release each counted value that the object holds: each
   counted field of a class (by its declared type after generic
   substitution) and each value of a counted Map. The release uses the
   §171 rule 7 description of the type. A class allocation carries a
   description of its counted fields, derived from the class layout;
   the runtime never infers a field kind from its bytes.
3. **One release per value.** A value that a free releases is not
   released again by a later sweep, and the reverse. A free of an
   object that a sweep reclaimed is the existing double-free check.
4. **A deterministic order.** A collection first finds every object it
   reclaims, then releases their counted values in increasing task id
   of the released frame (§169), then frees the allocations. A frame
   that this release frees with an unobserved exception traps with
   §116.1 rule 4. The first such frame in that order is the reported
   trap; the collection releases the rest before it reports. The order
   does not depend on the sweep order of the allocation table.
5. **Where the trap appears.** A trap of rule 4 inside
   `Context.collect()` of a script is a trap at the position of that
   call, in each tier. A trap inside a host collection call is reported
   through the Context trap state that the host reads after the call
   (the same state as a trap inside a host checkpoint, §168).
6. **An unfinished frame.** A release of rule 2 does not stop a frame.
   A registered frame stays a collection root while it is unfinished
   (§94). If the release takes the last count of a frame, the frame is
   freed when it finishes, as §70 rule 3 states for any last release.
7. **The interpreter collects as the native tiers do.** The interpreter
   does not root a script allocation permanently. Its roots are the
   roots that the native tiers have: live locals and temporaries of
   each active and suspended frame, module globals, registered frames,
   and host roots. A generator frame is a root only through a reachable
   value (an iterator in a root), as in the native tiers; the
   interpreter removes the registry entry of an unreachable generator
   at the collection. After a collection, the live allocation count of
   a script object is the same in the three tiers.
   The interpreter computes its roots when a collection runs, from its
   frame and scheduler state; it does no root work per instruction.
   A value is a root of a frame only if it is live before the current
   instruction of that frame: the result of an instruction that has not
   finished (a call in which the collection runs) is not a root, even
   if its slot holds a value of an earlier iteration. The test is per
   value: a slot is a root only if the live interval of the value that
   the slot holds contains the point, not the interval of another value
   with the same origin (a suspend or merge parameter of an earlier
   iteration).

   **One root definition for every tier.** The root set of a frame is
   the union of two sets, each derived from the LIR in every tier:

   - the values that the shared LIR liveness plan states as live at the
     point of the frame (the interpreter walk and the native shadow
     slots); and
   - the frame-lifetime storage of an unfinished coroutine: its
     parameters, the environment of each function value that it
     defines, its stable values (one set, derived from one LIR fact,
     that the dev JIT and the C tier both use), and its frame locals
     (§68.2 rule 8b).

   A native tier does not clear frame-lifetime storage while the
   coroutine runs or is suspended, because a function value or an
   address can point into it (a closure that captures a closure, passed
   to an awaited call, reads the caller's environment field through a
   pointer). When the coroutine finishes, each tier clears its
   frame-lifetime storage, so a finished frame roots no value; its
   completion holds its result. The interpreter roots the same
   frame-lifetime storage for an unfinished coroutine. A review of a
   tree that cleared native frame fields at the end of each value's
   interval predicted, by code reading, that a closure that captures a
   closure and is passed to an awaited call reads a cleared environment
   field; the run of that program (`a342`) did not reproduce it. The
   frame-lifetime form removes the question: no native frame field is
   cleared while a pointer into the frame can exist.

   The collection releases a handle leaf through a release consumer that
   the tier supplies, with a task-id resolver for rule 4. A native tier
   supplies the runtime registry (`Context::async_release`, §169 task
   ids). The interpreter supplies its own registry: its packed handle is
   an interpreter frame address that the runtime registry does not hold.
   The collector receives the consumer before it frees any storage, and
   it never resolves a handle by the runtime registry alone. Round 1
   measured at `dbccd4d8` that a runtime release of an interpreter
   handle releases nothing.

   The class field description of rule 2 and the Map value description
   of rule 1 are new facts of the allocation form: a class allocation
   and a Map header carry them (or a key to them) when they are
   created. Their absence at `dbccd4d8` is the work of this section.
8. **Context destruction does not trap.** Destruction frees storage and
   releases nothing that can report. This is the current behaviour; a
   destroyed Context has no observer.
9. **A total check.** Each Map operation of rule 1 on a counted value
   type carries its count action in its LIR instruction (§171 rule 9),
   and the verifier derives the description from the type and
   compares. A class whose layout has a counted field has a field
   release description; the verifier compares it with the field offsets
   and types that the class layout computes separately. A unit test
   builds a description with a wrong offset and one that omits a
   counted field, and reads each check failure. `Context.collect()`
   carries a trap site, and the verifier checks it. Each instruction
   that can release carries a trap site (§171 rule 10).
10. The three tiers (dev JIT, C AOT, interpreter) give the same counts,
   the same trap, and the same output.

### 172.2 Acceptance

1. Red first, at the contract pin. Accept entry `a341`: a counted Map
   (a handle, a handle array, a nested array, a `FixedArray`) through
   `set`, `set` over a key with a distinct and with the same value,
   `delete`, `clear`, `getOr`, and `values()`; a class field holder
   made unreachable and collected; the program prints a value after
   each await, so a freed and reused frame changes the output. Trap
   entries, each a failed task that the program never observes:
   `t94` (removed by `Map.delete`), `t95` (removed by `Map.clear`),
   `t96` (replaced by `Map.set` over its key), `t97` (held in a field
   of an unreachable object; `Context.collect()` releases it; the trap
   is at the collect call), `t98` (two unreachable objects with failed
   tasks; the reported trap is the task created first). `t99` (a callee
   that collects, called in a loop, while the previous iteration's
   discarded result holds a failed task; the trap is at the second
   collect, in each tier; rule 7). Reject entry
   `r388`: `Map.forEach` over a counted value type (rule 1). Each with its
   measured `tsc` header and its pin result.
2. Unit tests: each row of the measurement tables 1 to 3 with its zero
   control requires zero retained tasks after the release point; a
   host collection trap test; a rule 3 test (free, then collect; and
   collect, then a free attempt); a rule 7 test of the live
   allocation count in the three tiers.
3. The interpreter corpus test time: the best of three runs is at most
   1.05 times the best of three at `9647c72a` (alone). Measured on the round 6 tree:
   9.240 s against 8.964 s (1.031); the median (1.098) includes one
   17.081 s run, and a separate run gives a wall time of 9.770 s against
   9.859 s. The collection cost of a heap with no counted value: the median is
   at most 1.05 times the pin median (release, best of three, alone),
   for the measurement item 5 shapes. The async-cost benchmark: at
   most 1.05 times the pin median.
4. The LIR text golden moves for the new entries. No other `.expected`
   golden moves.

### 172.3 Open

1. Closed by §177 rule 1. The cause is the operation table that the
   LIR builder rebuilt for each call instruction.
2. The frame-lifetime half of the suspension total check seeds each
   suspension with the lifetime set, so a missing native write of an
   `env_v`, `stable_v`, or frame-local field cannot fail it; only the
   parameter has a write check (`codegen/src/lower/func/frame_root_tests.rs`).
   The premature-clear loop skips block-parameter values and branch
   edges, and the C finish-clear check tests for presence, not per exit.
   No program reaches this; it is a check defect under core principle 9.
3. The interpreter is not in the total check; its root parity is tested
   by behaviour (`codegen/tests/interpreter_roots.rs`).
4. A derived address that alone crosses a suspension (for example a
   field address of a value-class temporary) is not in the stable set
   (contrived; code reading, not measured).
5. The dev JIT does not root the `sort` scratch values: a comparator
   that replaces the receiver values and then calls `Context.collect()`
   gives UseAfterDelete in the dev JIT; the interpreter prints `1,2,3`
   (contrived; present before §172).
