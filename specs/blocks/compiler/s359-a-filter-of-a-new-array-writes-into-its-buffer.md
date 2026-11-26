<!-- §359 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 359. A `filter` of a new array writes into its buffer

*(Added 2026-11-26.)* Origin: the owner decision of 2026-11-26, and
the measurements N1 and N2 of
`specs/tracking/non-escaping-allocations.md`.

Problem, measured at `a3ead965` in the ship tier:

1. **A chain of array methods allocates one buffer for each step.**
   In the `callbacks` workload each round maps 1,000,000 `i32` into a
   new array and filters that array into a second new array. No
   other code reads the first array.
2. **A hand change of the emitted C that lets the filter write into
   the buffer of its input changes the time from 36.09 to 24.91 ms**
   (N1), with the collection on or off. The live bytes after the
   call go from 28,194,688 to 24,194,640. `callbacks` is the workload
   with the largest ratio to C in the benchmark record (2.79x).
3. **The form has one site in the benchmarks and 12 in the corpus**
   (N2).

### 359.1 Rules

**The form**

1. **An in-place `filter`.** A static `filter` call (§8.1d B: a
   dynamic array receiver, and a callback that is a named function
   or a lambda literal) is in place if its receiver is a new array
   with one reader (rules 2 to 4). Its result is the array of its
   receiver: the header and the buffer are those of the receiver,
   and the call allocates nothing.
2. **A new array.** The receiver is one of these two forms.
   a. The receiver expression is itself a static `map` or `filter`
      call on a dynamic array, or an array literal.
   b. The receiver is a `const` local of the same function. Its
      initializer is a form of rule 2a. The `filter` call is the one
      read of the local in the function, a lambda body included.
3. **One evaluation.** In form 2b, each loop that contains the
   `filter` call also contains the declaration of the local. A
   declaration outside a loop with the call inside it is not the
   form: the call runs again on its own result.
4. **Not in the form:** a receiver that is a parameter, a `let`
   local, a field, a global, a `FixedArray`, or the result of
   another method or function; a callback that is a function value;
   a function that is a generator or an async function.
5. **A program cannot observe the form,** apart from its time and
   the bytes that the Context holds. Each output and each runtime
   error of a program is the same as with a `filter` that allocates.

**What the loop does**

6. **The traversal does not change** (§8.1d B, §117): the bound is
   fixed before the first call, each step reads the element at the
   input position, and the index argument is the input position.
7. **The loop keeps its own count of the kept elements.** It writes
   a kept element at that count. The count is never above the input
   position. The loop does not write the length of the array.
8. **After the loop, the array takes the count as its length, and
   each byte of the buffer from the new length to the former length
   is zero** (§80 rule 1). An exit of the loop by a raise or by a
   runtime error does not run this step: no value names the array
   after such an exit.
9. **A collection during the loop reads a valid array:** the length
   is the former length, and each element is a value that the array
   held at the start.
10. **A store of a kept element is a store into a block** for §356
    rule 13: it carries the barrier of rule 10 of §356 where the
    element type can hold a reference. The step of rule 8 removes
    references and needs no barrier.
11. **The handle fact of §344 is that of the receiver** (§344 rule 4
    states that a `filter` takes the fact of its source).

**The form in LIR**

12. **LIR states the in-place form.** The lowering decides it, and
    the three tiers execute the same instructions (§68.2 item 10).
    An in-place `filter` has no `ArrayWithCapacity` and no
    `ArrayPushReserved`. The instructions that write a kept element
    and that end the loop (rule 8) carry the lifetime sites of §120.
13. **The decision has a second derivation.** The check of the LIR
    facts against HIR (`codegen/tests/support/lir_facts`) derives
    the in-place calls from HIR by rules 2 to 4, and it compares
    that set with the calls that LIR lowers in place. It requires
    one capacity allocation and one reserved push for each other
    static `map` or `filter`.
14. **The form has a reference.** A test lowers each corpus entry
    that has an in-place `filter` two times: with the decision, and
    with each `filter` as one that allocates. It runs both and
    compares the outputs. The check does not read the decision: a
    decision that is wrong for a program of the corpus changes an
    output. The switch that turns the decision off is for tests
    only.

### 359.2 Acceptance

1. **Output.** An accept entry with these cases, equal in the three
   tiers and to its golden:
   - form 2a after a `map`, after a `filter`, and after an array
     literal; form 2b;
   - element types `i32`, `f64`, `string`, and a reference class (a
     `filter` is not defined for an array of a value class, Q22);
   - a callback with the index parameter, where the output shows
     that the index is the input position;
   - a lambda with a capture;
   - a `filter` that keeps each element, one that keeps none, and
     an empty receiver;
   - a callback that throws at an element in the middle, inside a
     `try` of the same function;
   - not in the form, with the receiver read after the call: a local
     with a second read; a local that a lambda captures; a
     declaration outside a loop with the call inside; a `let` local;
     a parameter.
2. **Red first.** The entry of item 1 passes at the pin: the form is
   not observable (rule 5). The Red evidence is the check of rule
   13: it fails at the pin for the entry.
3. **Rule 3 has a control.** A test changes the decision so that it
   ignores rule 3. The loop case of item 1 then prints a different
   value.
4. **Rule 8.** A test reads the buffer after an in-place `filter`
   that removes elements: each byte past the new length is zero. A
   debug build runs a whole collection after the call with no
   violation of the array tails.
5. **Rule 9.** The entry of item 1 has a callback that allocates. It
   passes the collection stress sweep in mode 2 and in mode 3 at
   each step budget.
6. **Rule 10.** A test in `codegen/tests/mark_barrier_points.rs`: an
   in-place `filter` of a large array of objects during a mark. The
   test fails if the store carries no barrier.
7. **The emitted C** of an in-place `filter` has no call of an array
   creation entry, and no growth branch.
8. **`callbacks`.** The time of the workload in the ship tier and in
   the development tier, before and after, measured and recorded.
   The expected figure of the ship tier is about 25 ms. The
   poll-site count of the workload stays 2
   (`codegen/tests/return_path_polls.rs`).
8a. **Rule 14.** The test of rule 14 passes, and it states the
    count of the entries that it runs. Control: with the decision of
    item 3 (rule 3 ignored), the test reports the entry of item 1.
9. The LIR text golden moves only for functions with an in-place
   `filter`. The report names each one.
10. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
    passes.

### 359.3 Sections this one amends

- §8.1d B: a static `filter` has two lowerings.
- §356 rule 13: the total match of `block_store` takes the new
  instructions.
- The check of the capacity allocations in
  `codegen/tests/support/lir_facts` (rule 13).

### 359.4 Not in this section

- An in-place `map` whose result has the element type of its
  receiver.
- A receiver that is the result of a runtime array method (`slice`,
  `concat`, and the others): LIR does not state which of them
  return an array that no other value names (N2).
- A general proof that an allocation does not escape, a class
  instance with no heap allocation, and a release at a last use.
- `Context.move` and `Move<T>`.
