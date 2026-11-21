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

### 344.5 Phase Review

Form.

- The carrier is the class id. `CLASS_ARRAY_NO_HANDLE` is the header
  of an array whose elements hold no handle, and
  `CLASS_ARRAY_DATA_NO_HANDLE` is its data buffer. The header keeps
  the fact while the array has no buffer and across a growth.
  `array_data_class` gives the buffer class at each buffer allocation.
  `class_holds_no_handle` lists the buffer class, so the mark has no
  branch for arrays.
- `reported_class_id`, `is_array_class`, and `is_array_data_class` are
  the one test of "this is an array". A host visitor receives the
  class ids that it received before.
- The predicate is `holds_no_handle` in `codegen/src/layout.rs`. It
  does not reuse `managed_words`: that function lists the handle types
  and its default is "no handle".
- The LIR does not change. Each tier takes the fact from the element
  type that the creation instruction carries, and selects one of two
  runtime entries. `map`, `flatMap`, and `Promise.all` take the fact
  from the compiler. A received array takes it from one flag of its
  read record.
- The reuse of a free block zeroes each class but a string, by a
  predicate of its own (`class_written_in_full`).

Measured.

| Item | Result |
|---|---|
| Words that one collection reads, live `i32[]` and `f64[]` of 4 MB and of 8 MB | 10 and 10 (dev tier), 12 and 12 (ship tier); at `aef4cdd3`: 1,048,586 and 2,097,162, and 1,048,588 and 2,097,164 |
| `callbacks`, `cross-language`, two passes, `main` against this section | ship tier 37.12, 37.07 ms against 36.65, 36.73 ms; dev JIT 257.5, 257.4 ms against 256.9, 256.7 ms |
| `particles`, `sort`, `tree`, both tiers | equal to `main` in the noise of the two passes |
| `f64[]` probe, ship tier, a live 4 MB array and garbage in a loop | 706 to 712 ms before, 281 to 284 ms after |
| `perf-gate`, two passes, `main` against this section | `a22` ship tier 1.33x, 1.33x against 1.34x, 1.33x; `a22` dev JIT 19.85x, 19.84x against 20.07x, 19.91x |
| `collect` ship tier (§344.4) | 36.2 ms against 49.7 ms; `perf-gate` 0.88x, 0.86x against 1.19x, 1.18x |
| Stress sweep | dev JIT 591 of 591, ship tier 589 of 589, interpreter 486 of 486 |
| An array of references through a collection, a growth, and a collection | `150 78675 78675 78675` in the three tiers, in mode 1 and in mode 2 (expected value by hand) |
| An array of references created with the "no handle" fact | its element is collected in both allocator tiers (the firing control) |

One `perf-gate` pass of `main` was void (25.3% spread on the C
reference of `collect`) and ran again.

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 7
MINOR. No creation or copy site gives "no handle" to an array that can
receive a handle.

| Finding | Close |
|---|---|
| A received array and a `Promise.all` result took the default fact | each takes the fact of its element type |
| §342 did not state the amendment | §342 rule 10c, and §342.5 item 3 |
| The two no-handle entries had no direct test; no test read the fact in the dev JIT; `ArrayElements::and` had no test | three tests; the dev run returns the live array facts in its accounting |
| A doc comment, and the class id in the mark trace | corrected; the trace prints the reported id and the fact |
| Array covariance and a reload that changes an element type were not verified by reading | the checker rejects each widening of an element type (S011, S100); a reload refuses the changed global (`DeclarationChanged`), with a test |

`runtime/src/arrops.rs` has 2,675 lines; it had 2,623 at `ca3fc9ed`
(§5.y). `runtime/src/assocops.rs` (1,997), `codegen/tests/lir.rs`
(1,992), and `codegen/src/lower/mod.rs` (1,986) are near the limit.

`tools/hygiene.sh`: exit 0. `tools/gate.sh full` on the tree before
this record: exit 0, debug 3924 passed, release 3905 passed, 0
failed, 5 ignored in each. No golden moved.

### 344.6 Open

1. A received 4 MB array has no test of the words that a collection
   reads: no fixture returns a large array. The mark is the same code
   as for acceptance 2.
2. `runtime/src/arrops.rs` is over the limit of §5.y.

