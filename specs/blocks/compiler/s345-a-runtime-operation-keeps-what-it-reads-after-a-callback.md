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
   temporary of the operation. *(Amended 2026-11-22, §351 rule 6:
   the bound holds on a chain of returns and on a chain of resumes in
   one drain.)*
4. **The ship tier gives its exit count to a test host.** A runtime
   entry outside the `subscript_rt_ctx_` family returns
   `outermost_exit_count()`. The host header does not declare it. The
   stress sweep reads it after the last script exit and before the
   release of the Context, by a channel that is not the stdout of the
   script. For each entry that the three tiers run to the end with no
   trap, the three counts are equal. *(Amended 2026-11-22 by §349
   rule 16: the sweep does not compare the count of a program that
   calls `Worker.receive`.)*

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

### 345.5 Phase Review

Form.

- `OperationRoot` (`runtime/src/context/operation_root.rs`) pushes a
  shadow frame over a range of words and pops it when it drops. A
  script exception and a trap set a flag and return, so the drop runs
  on each exit of an operation.
- `filter` and the `find` family write the element into one slot before
  each callback. `Map.groupBy` roots its element buffer, which has a
  fixed length. `sort` roots its two scratch buffers, only if the
  array can hold a handle (§344); a swap of the two `Vec` values does
  not move their storage. `map` uses the same guard for its result.
- Rule 2 found each of the five operations local, so each is
  corrected.
- `sort` and `group_by` are in modules of their own
  (`runtime/src/arrops/sort.rs`, `runtime/src/assocops/group_by.rs`).
  `runtime/src/arrops.rs` went from 2,675 to 2,575 lines, and
  `runtime/src/assocops.rs` from 1,997 to 1,874.
- `subscript_rt_outermost_exit_count` and
  `subscript_rt_automatic_collection_count` are runtime entries outside
  the `subscript_rt_ctx_` family. The host header does not declare
  them. A post-run hook of the stress sweep writes the exit count to a
  report file.

Measured. Red at `53134b4c`, with the binary of that commit:

| Entry | Golden | Dev JIT at the pin | Ship tier at the pin |
|---|---|---|---|
| `a493` (`filter`) | `filter 10,32,43,` | runtime error, use of a deleted allocation | `filter 31,0,1,` |
| `a494` (`find`, `findLast`) | `find 32`, `findLast 17` | runtime error, use of a deleted allocation | `find -25098160`, `findLast -25097904` |
| `a495` (`Map.groupBy`) | `k0 10,32,`, `k1 21,43,` | `k0 15,24,`, `k1 13,2,` | `k0 29,44,`, `k1 29,3,` |
| `a496` (`sort`) | `sort 10,20,30,40,50,60,` | runtime error, use of a deleted allocation | `sort 0,1388380240,31,32,…` |

The ship values at the pin are addresses and change from run to run.
`a495` is Red at the pin with a template string as the key; with a
conditional string as the key it printed the golden there, and the
cause is not known.

| Item | Result |
|---|---|
| The four programs with no `Context.collect()`, mode 2, three tiers | output equal to mode 0; automatic collections 162, 237, 165, and 444, against 3 outermost exits in each |
| Control: a program with no poll | 3 automatic collections and 3 outermost exits |
| A callback that raises | the shadow depth after the operation equals the depth before it |
| Exit count in the stress sweep | 480 entries compared: 402 in three tiers and 78 in two; no difference |
| Control: an export call with no bracket | a lower count than the call with the bracket (3 against 4) |
| Stress sweep, 598 entries | dev JIT 595 of 595, ship tier 593 of 593, interpreter 490 of 490 |

The count of collections over the count of exits shows that a poll in
the script call collected. It does not show that the poll was in the
callback: each `main` also has loops that allocate. The runtime test
`runtime/tests/operation_roots.rs` uses Rust callbacks, which hold no
root of their own, and fails without the guard.

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 5
MINOR, each closed. For a program that does not change its receiver,
each root is valid and balanced, and no result changes. The reviewer
evaluated the four goldens by hand.

| Finding | Close |
|---|---|
| The module doc of the poll pass and §342 rule 11d stated the wider bound; three open items stayed open | corrected; §342.5 items 4 and 5 and §323.4 item 2 state the close |
| `subscript_rt_outermost_exit_count` had no direct test | a runtime test |
| The sweep did not compare the dev JIT with the ship tier where the interpreter gives no count | each pair of counts is compared; each arm that compares nothing states its reason |
| The test with no explicit collection did not show a collection on the ship tier | the count over the exits, in the three tiers |
| A comment named only `Context.collect()` | corrected |

A separate review of `aef4cdd3` gave the correction of §342 rule 1: the
rule states what a collection does, and Q6 states what `Context.free`
does.

