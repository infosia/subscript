<!-- §358 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 358. `Context.free` releases an array

*(Added 2026-11-26.)* Origin: the owner decision of 2026-11-26, and
the measurement N1 of `specs/tracking/non-escaping-allocations.md`.

Problem, measured at `a3ead965`:

1. **A program cannot release an array.** `Context.free(a)` with
   `a: i32[]` gives `error[S100]: type mismatch: the argument expects
   `object`, got `i32[]``. The checker accepts a reference-class
   instance, a `Map`, and a `Set`. At the manual level and at the
   host-timed level of invariant 2, the memory of an array stays
   until a collection or the Context release.
2. **`tsc` accepts the call.** An array is assignable to `object`.
   The rejection is a divergence with no stated reason.
3. **The largest blocks of a program are array buffers.** In the
   `callbacks` workload with the collection off, the release of each
   array at its last use lowers the live bytes after the call from
   164,196,320 to 4,194,400 (N1). With the collection on it removes
   the four automatic collections of the call.

### 358.1 Rules

1. **`Context.free(value)` accepts a dynamic array.** The argument
   type is `T[]` for each element type `T` that is not a counted type
   (§171). The types that the call accepted before this section do
   not change.
2. **The call releases the array header and its buffer, at once.**
   An array with no buffer releases the header only. The call
   releases a buffer exactly one time on each path, also where the
   array grew before the call. Each buffer of an array is a block of
   the Context: no array header refers to other memory.
3. **The call does not release an element.** An element that is a
   reference stays until a collection, or until the program releases
   it. This is the rule of a field of a freed object (§172 states the
   exception for a counted field; rule 5 excludes counted elements
   here).
4. **The requested-bytes count of §346 goes down by the request of
   each block that the call releases.** A loop that makes an array
   and frees it runs no automatic collection.
5. **A counted array is rejected.** An array whose element type is a
   counted type has a holder count, and its last holder releases it
   at once (§171 rule 4). `Context.free` of such an array is a
   compile error. The diagnostic names the element type and states
   that the last holder releases the array.
6. **An array with a native-call borrow stops the program.** If a
   callback frees an array while a foreign call holds its pointer
   (§323), the call gives the runtime error of a growth during a
   borrow (`ArrayStorageBorrowed`), in the development tier and in
   the ship tier, with the position of the `Context.free`.
7. **Each other use after the call is Q6.** A read through a second
   reference, a second `Context.free`, and a free of an array inside
   its own `for…of` are undefined in both tiers. With the freed-handle
   diagnostics of §8.1a on, the development tier and the interpreter
   give the traps of Q6 for an array as they do for an object.
8. **A string is not accepted.** A string is a value with no
   identity: a copy shares the block, a literal is one block for each
   evaluation of the Context, and some string operations return
   their receiver. `tsc` also rejects a string for `object`. A
   nullable array, a `FixedArray`, a value class, a closure, a
   handle, a generator, and a regex value stay rejected.
9. **A free during a mark or a sweep in steps is valid** for the two
   blocks of an array, small or large (§356 rule 9, §357 rule 8).
10. **W002 covers an array** with no change of its rule: it follows
    local names.
11. **The three tiers agree.** The interpreter, the development
    tier, and the ship tier release the same blocks, and each prints
    the same output.

### 358.2 Acceptance

1. **Red first.** An accept entry frees an array of numbers, an
   array of references to objects (the objects stay valid and the
   entry reads them after the call), an array that grew past its
   first capacity, an empty array, and an array in a field of an
   object (the program frees the array, then the object). At the pin
   the checker rejects the entry.
2. **Bytes.** A runtime test or a ship test for each tier that can
   read the live bytes: after the free of an array of 1,000,000
   `i32`, the live bytes equal the value before the array. One case
   has a small buffer (a size class) and one a large block.
3. **Rule 4.** A test in the form of §346 acceptance 4: a loop that
   makes an array and frees it runs no automatic collection, with a
   headroom of 64 KiB and a header and a buffer that request 64 KiB
   together. Control: the same loop with no free runs at least one.
4. **Rule 5.** A reject entry: `Context.free` of a `Promise<void>[]`.
   Its header states what `tsc` does, measured.
5. **Rule 6.** A test in the form of the native-call tests of §323: a
   callback frees the array of the call. The two native tiers give
   the same kind, message, and position.
6. **Rule 7.** Two trap entries with the diagnostics on, in the form
   of `t22` and `t23`: a second free of an array, and an element read
   after the free.
7. **Rule 8.** A reject entry: `Context.free` of a string. A unit
   test for a nullable array and a `FixedArray`.
8. **Rule 9.** The accept entry of item 1 passes the collection
   stress sweep in mode 2 and in mode 3 at each step budget, with no
   stop of a check of §356 rule 20, §356 rule 20a, or §357 rule 15.
   A runtime test frees an array with a large buffer while the mark
   has that buffer as its divided scan.
9. **Rule 10.** A warn entry or a unit test: a read of a local array
   after its free gives W002.
10. The matrix of `compiler/tests/generic_tsc_matrix` records that
    the checker and `tsc` agree for the array role of `Context.free`.
11. The texts that state what `Context.free` accepts are correct: Q6
    of `specs/blocks/collisions.md`, `specs/blocks/corpus.md`,
    `specs/blocks/stdlib.md`, the API text and the language
    reference (generated), the three tutorials, and `README.md`.
12. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
    passes.

### 358.3 Sections this one amends

- Q6 of `specs/blocks/collisions.md`: the argument is a
  reference-class instance, a `Map`, a `Set`, or a dynamic array of
  an uncounted type.
- §171: rule 5 of this section adds the rejection for a counted
  array. The rules of the holder count do not change.
- §323: rule 6 of this section adds the free to the operations that
  a borrow stops.

### 358.4 Not in this section

- A release that the compiler adds at a last use. N1 measured it: a
  gain in memory for a large buffer, and a loss in time for a small
  object.
- A release of the elements of an array (a deep free).
- A string.

### 358.5 Phase Review

Form.

- The checker gives the first argument of `Context.free` its own
  type as the parameter if its apparent type is a dynamic array
  (`check_free_argument`). A counted element type is the rejection
  site `ContextFreeCountedArray` (S014). A type parameter element
  takes the result of its instance (§143 rule 2a).
- One runtime entry does the release: `Context::free_value`.
  `subscript_rt_delete` calls it for the two native tiers, and the
  interpreter calls it directly. No generated code changed.
- The array step runs after the lookup that `delete` already does,
  on a live array header: a native-call borrow gives the trap and
  keeps both blocks; if no borrow exists, the buffer is released and
  then the header. An array header is always a small block of the
  ship arena.
- Each `data` word of an array header comes from an allocation of
  the Context (`array_with_capacity_of`, `array_from_bytes`, the
  growth).

Measured.

| Item | Result |
|---|---|
| The accept entry `a511` at the pin | S100 at each of its five `Context.free` calls |
| Live bytes and requested bytes after the free of an array of 0, 16, and 1,000,000 `i32` | Equal to the values before the array, in the dev kind and in the ship kind |
| A loop that makes an array and frees it, headroom 64 KiB | No automatic collection; the control with no free runs collections. With a buffer of 64 KiB alone, the header passes the headroom and each of 512 iterations collects |
| A free of the array of a foreign call in a callback | `ArrayStorageBorrowed` at the position of the free, equal in the two native tiers |
| `r564` (a `Promise<void>[]`) | S014; `tsc` accepts it |
| `r565` (a string) | Rejected; `tsc` gives TS2345 |
| Stress sweep, mode 2 and mode 3 at budgets 64, 16, and 4 | 611 runs equal in each, no stop of a check |
| `tree` in the ship tier, median of 10, a release entry that calls `delete` directly | 95.1 ms |
| The same with a liveness lookup before `delete` (the first form of this section) | 108.1 ms |
| The same with the landed form | 93.0 ms |

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 8
MINOR.

| Finding | Close |
|---|---|
| The message for an array of generators had the hint of the array methods | The message has no hint; a unit test reads it |
| A branch of the release for a large block had no array that reaches it, and two comments did not agree with it | The branch is removed; the comments state the code |
| The test of a free during a divided scan did not name the buffer | It reads the divided scan before and after the free |
| The test of rule 6 stated that both blocks stay, with no check | A runtime test reads the liveness of each block, with a control |
| The test that an element stays had no control | The control frees the element and reads it as not live |
| A test name and three sentences of the tutorials | Corrected |
| The Red record of §311 in `specs/tracking/divergence-alternatives.md` has no row for the new rejection site | Not a defect: the record states the tree at its own commit |

The first full gate of this section failed with three tests. Two
tests of `generic_tsc_matrix` read a phrase of Q6 that the new text
of Q6 does not have: they read "reference-class instance". One test
of `codegen/tests/lifetime_operands.rs` had an array as a subject
that no program can release: the array rows are the test
`for_of_array`, which reads the trap of a `for…of` after the free.

`tools/gate.sh full` on the tree before this record:

```text
gate full 9dfa247c977a711a00c33a1b296226a0399260ff dirty:43 debug 4247/0/10 release 4228/0/10 skips 2/0 clippy 4/10/13 goldens-moved 0 exit 0
```

`tools/hygiene.sh`: exit 0.

### 358.6 Open

1. A free of an array inside its own `for…of`, and a free while a
   suspended frame holds a cursor of the array, are Q6. With the
   diagnostics off no tier reports them.
2. The time of a free in the dev kind (the allocation map) is not
   measured.
3. A program cannot release a string, a closure environment, or a
   regex value before a collection.
