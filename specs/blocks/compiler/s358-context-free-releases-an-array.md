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
