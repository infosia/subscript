<!-- §345 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 345. A runtime operation keeps what it reads after a callback

*(Added 2026-11-21.)* Origin: two reviews of `aef4cdd3` by another
agent, and §342.5 items 4 and 5.

Problem:

1. **A native operation loses an element that a callback removes.**
   `filter`, `find`, `findLast`, `Map.groupBy`, and `sort` with a
   function-value callback hold an element in a Rust local while the
   callback runs (`runtime/src/arrops.rs`, `runtime/src/assocops.rs`).
   If the callback removes or replaces that element in the receiver, no
   root holds it. Before §342 the callback also needed an explicit
   `Context.collect()`. Since §342 an allocation in the callback
   reaches a poll. Measured at `ca3fc9ed` with an explicit collection
   in the callback: the interpreter prints `findLast 32`, and the ship
   tier prints `findLast 2055278416` with no error. The shape is rare:
   a callback that changes the array it iterates.
2. **§342 rule 11d states more than the code gives.** One runtime
   operation can allocate in proportion to its input, and no poll is
   in a runtime loop. Measured: a `split` into 10,000 pieces made
   10,014 allocations and no collection, and the flag was set at the
   return.
3. **The stress sweep does not compare the exit count of the ship
   tier** (§342.5 item 5). The Context holds the count; the ship test
   host has no way to read it.

Owner decision, 2026-11-21: a rare shape that is hard to correct is
recorded only.

### 345.1 Rules

1. **A runtime operation roots what it reads after a callback.**
   Before a runtime function calls a script callback, each managed
   reference that the function reads after the callback returns is in
   a root set. The root ends when the operation ends, by a return or
   by a trap.
2. **Rule 1 applies where the change is local.** It applies to an
   operation if a root for the scope of that operation closes the
   defect, with no change to the form of the operation. `filter`,
   `find`, `findLast`, `Map.groupBy`, and `sort` are the candidates.
   An operation that needs more is not changed; §345.4 records it with
   its measured result.
3. **The scope of §342 rule 11d is LIR control flow.** *(Amends §342
   rule 11d.)* Each path through LIR control flow reaches a poll, or
   the outermost exit, after a bounded count of LIR instructions that
   can allocate. One runtime operation can allocate in proportion to
   its input; the flag that it sets is served at the first poll after
   it returns, or at the outermost exit. No poll is in a runtime loop:
   a poll there needs a root for each input, partial result, and
   temporary of the operation.
4. **The ship tier gives its exit count to a test host.** A runtime
   entry outside the `subscript_rt_ctx_` family returns
   `outermost_exit_count()`. The host header does not declare it. The
   stress sweep reads it after the last script exit and before the
   release of the Context, by a channel that is not the stdout of the
   script. For each entry that the three tiers run to the end with no
   trap, the three counts are equal.

### 345.2 Acceptance

1. **The corrected operations.** For each operation that rule 2
   corrects, one accept entry (`a493` and up): the callback is a
   function value, it removes or replaces elements of the receiver,
   and it calls `Context.collect()`. The entry reads each result
   element. Red at `53134b4c` in the two native tiers; the interpreter
   prints the golden there.
2. **No explicit collection.** A three-tier test runs the same shapes
   with no `Context.collect()` in mode 2, and compares with the output
   of mode 0.
3. **The trap path.** A callback that raises ends the operation; the
   root set has no entry of the operation after it (a runtime test
   reads the root count, with a control).
4. **Exit count.** The stress sweep compares the three tiers (rule 4).
   Firing control: a ship run with one export call that has no bracket
   gives a lower count.
5. The stress sweep of §342.2 item 1 and `tools/gate.sh full` pass.

### 345.3 Sections this one amends

- §342 rule 11d (rule 3 here), §342.2 item 8 (rule 4 here), and
  §342.5 items 4 and 5.
- §323.4 item 2 closes: rule 2 corrects `sort`.

### 345.4 Open

Rule 2 corrected each of the five operations, so no operation is
recorded here.
