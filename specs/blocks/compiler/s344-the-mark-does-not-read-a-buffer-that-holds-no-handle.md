<!-- §344 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 344. The mark does not read a buffer that holds no handle

*(Added 2026-11-21.)* Origin: the benchmark comparison of the branch
`gc-auto` against `main` (`specs/tracking/automatic-collection.md`,
G6).

Problem, measured at `aef4cdd3`:

1. **A live array of scalars makes each collection cost its size.**
   The mark reads each word of an array data buffer and looks it up as
   an address. `class_holds_no_handle` is true for a string only. In
   `callbacks`, one collection read 524,294 words of a 4 MB `i32[]`,
   found 2 handles, and took 7.3 ms in the ship tier. The workload
   went from 37 ms to 87 ms in the ship tier and from 258 ms to 337 ms
   in the dev JIT. A 4 MB `f64[]` gave 13.5 ms for each collection.
   A large array of numbers is the main data of the hosts this
   language serves.
2. **A loop that only makes values has a poll.** An `ArrayLiteral`
   with a `FixedArray` result and an `AllocateClass` of a `@ValueType`
   class with an address result state "can allocate" (§342 rule 10).
   Neither allocates in the Context. The hot loop of `a22` got a poll,
   and the ship tier went from 1.34x to 1.44x of C in `perf-gate`.

### 344.1 Rules

**The handle fact of an array**

1. **An array states if its elements can hold a handle.** The fact
   belongs to the array for its whole life. The mark does not read the
   data buffer of an array whose elements cannot hold a handle. It
   marks the buffer as reached.
2. **The carrier does not change the layout.** `ArrayHeader`, the
   element access of the emitted C, and the `(pointer, count)` form at
   the C boundary do not change. Each place that tests "this is an
   array" accepts both facts.
3. **The compiler gives the fact at each creation.** A type holds no
   handle if it is a numeric type, `boolean`, an enum, a wire-mapped
   or literal-union string alias (an index, §24), `Date`, a value
   class whose fields all hold no handle, or a `FixedArray` of such a
   type. Each other type can hold a handle. The predicate is one total
   function over the type form, and its default is "can hold a
   handle".
4. **The fact follows the elements.** Growth keeps it. A copy, a
   slice, a concatenation, a `filter`, a reverse, and each other
   operation that makes an array from the elements of one array give
   the fact of the source. An operation that makes new elements
   (`map`, `flatMap`, `Array.from`, a literal, a spread) takes the
   fact of the new element type from the compiler.
5. **A runtime operation that makes an array gives the fact of what it
   stores.** A byte array holds no handle. An array of strings, of
   regex results, or of pairs can hold a handle.
6. **Nothing else about the scan changes.** A class instance, a
   closure environment, and a coroutine frame are read as before
   (§342.5 item 1).

**Value forms and the allocation fact**

7. **An instruction that makes a value does not allocate.** An
   `ArrayLiteral` whose result is a `FixedArray`, and an
   `AllocateClass` of a value class whose result is an address, state
   "cannot allocate" (§342 rule 10). The interpreter check of §342
   rule 10a holds for both. If the interpreter allocates in the
   Context for one of them, its form is wrong for this rule: report
   it, and do not weaken the check.

### 344.2 Acceptance

1. **Root safety.** The stress sweep of §342.2 item 1 passes in the
   three tiers. An array of references that states "no handle" loses
   its elements in that sweep, so the sweep is the check of rules 3 to
   5. A test builds that wrong form and expects a wrong result or a
   runtime error.
1a. **An array of references keeps its elements.** An array of
   reference objects is the only holder of its elements. A forced
   collection runs before a growth of the buffer and after it. Each
   element is intact in the three tiers. The same test with an array
   of numbers of the same size reads the same values.
2. **The mark skips the buffer.** With a live `i32[]` and a live
   `f64[]` of 4 MB each, the words that one collection reads do not
   grow with the array length. Red at `aef4cdd3`: the count grows with
   the length.
3. **The fact follows the elements.** For each operation of rules 4
   and 5, a test reads the fact of the result: an array of numbers
   and an array of reference objects through growth, `slice`,
   `concat`, `filter`, `map` to a reference type, and `map` to a
   number.
4. **`callbacks`.** Ship tier and dev JIT equal `main` in the noise of
   two passes, with the default mode.
5. **`a22`.** The loop of `propagate` has no poll (a LIR test), and
   `perf-gate` gives the ship tier the figure of `main` in the noise
   of two quiet passes.
6. `tools/gate.sh full` passes.

### 344.3 Sections this one amends

- §342 rule 10: rule 7 here adds two "cannot allocate" cases.
- §342.5 item 3 closes: the cost was the read of the scalar buffer.

### 344.4 Recorded, not changed

`collect` in the ship tier went from 38 ms to 53 ms. The cause is 90
automatic collections between the 105 explicit ones. The ship tier
counts the size-class capacity as live bytes (§18.2d), and the
workload requests one byte over each class size, so the capacity is
2.02 times the request and each round passes the threshold once. The
dev JIT counts the payload and runs no automatic collection. The
threshold follows the memory that the tier holds, so this section
does not change it. A host that collects at a point it selects sets
mode 0 (§342 rule 4a).
