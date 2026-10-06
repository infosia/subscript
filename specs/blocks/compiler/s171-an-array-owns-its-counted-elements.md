<!-- §171 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 171. An array owns its counted elements

*(Added 2026-10-06.)* Origin: §170.3 item 5. On 2026-10-06 the owner
selected the fix and divided it into two sections: this section (the
arrays and the async completion) and §172 (the reference-object
holders: a class field and a `Map` value).

Problem: §70 rule 3 says that the last holder's exit frees a frame. The
measurement round at `96f46f35` (`specs/tracking/s171-counted-holders.md`,
items 1 to 3) found that a count stays after every holder ends in these
shapes, in the three tiers:

- A copy of a handle array (a parameter, `const b = a`, a global, a
  field, a return value) retains each element that the array holds at
  the copy. A removal through any copy (`pop`, `shift`, `splice`) leaves
  the copy's count of the removed element with no release site.
- With one array holder and no copy, `shift`, `splice`, `fill`,
  `unshift`, and `copyWithin` keep a count. `copyWithin` copies the
  element bytes with no count operation. With a live local `h`,
  `copyWithin(0, 1)` over `[work(), h]` and an aliased `push(h)` give a
  use-after-delete in the dev JIT, and the tiers disagree (3/1/1 and
  2/0/0 tasks).
- The count operations know only a handle and an array whose element is
  a handle. A `Promise<void>[][]` (any depth) and a
  `FixedArray<Promise<void>, N>` keep one count after the holder ends.
- An async function whose fulfilled value holds a handle
  (`Promise<Promise<void>>`, `Promise<Promise<void>[]>`) keeps one count
  in its completion. The cache stores bytes and the frame free releases
  nothing.

A kept count keeps a failed task live, so its unobserved exception never
traps (§116.1 rule 4). The measured probe: a failed task removed by
`shift` and discarded gives no trap.

The round measured two forms. A form that gives each handle array one
owner (a borrowed parameter) does not close the class, because one
holder with no copy also keeps a count. It also rejects `a162`. This
section uses the other form: an array owns its element counts, and the
array has its own holder count.

The measurements cite `96f46f35`: the amendments of this contract change
specifications only, so the code of each amended pin is that tree.

### 171.1 Rules

1. A **counted type** is a handle type (`Promise<T>`), a dynamic array
   of a counted type, a `FixedArray` of a counted type, or an
   `IterResult<Y>` whose `Y` is a counted type. The rule is
   recursive, so it covers every depth. Each rule below that names a
   counted value applies to every counted type. A class field and a
   `Map` value of a counted type are §172.
2. **One owner per element.** A dynamic array of a counted type owns one
   count of each element that it holds. A copy of the array reference
   does not change an element count.
3. **The array has a holder count.** Every dynamic array has a `u32`
   holder count at payload offset 32. The array payload is 40 bytes
   (`ArrayHeader` and the emitted `SsArrayHeader`); the offsets 0, 8,
   16, and 24 do not move. A new array starts with a holder count of
   one. For a counted array, §70 rule 2 applies to the array: a copy
   increments the holder count, and a scope exit decrements it. The
   operations of §70 rule 2a (one store path and the verifier) apply to
   the array as they apply to a handle. An array of an uncounted type
   does not read or write the field.
4. **The last holder frees the array.** If the holder count of a counted
   array reaches zero, the runtime releases each element (rule 7) and
   then frees the array allocation, at that decrement, with no
   collection (§70 rule 3).
5. **Each mutation moves counts.** On a dynamic array of a counted type:
   - A store into a slot (an index store, `push`, `unshift`, the
     inserted items of `splice`, each slot of `fill` and `copyWithin`)
     acquires one count of the stored value. A fresh owner transfers its
     count. A store that replaces a value releases the replaced value
     after the new value is acquired.
   - A removal (`pop`, `shift`, the removed items of `splice`) transfers
     the element count to the result. If the program discards the
     result, the result is released at the statement.
   - An operation that only reorders the elements (`reverse`) changes no
     count.
   - An operation that makes a new array from elements (an array
     literal, `Array.of`, a spread literal, `Array.from`, `slice`,
     `concat`) acquires one count of each element for the new array. A
     fresh owner transfers its count.
   - An operation that keeps a reference to the array while user code
     runs holds the array: it increments the holder count at its start
     and decrements it at each exit. A `for…of` over a counted array is
     one; its loop holds the subject array through the normal exit,
     `break`, `return`, and an exception exit. Without the hold, a body
     that releases the last other holder (a reset of a module global)
     frees the array while the loop reads it.
   - An element read (an index read, `at`, a `for…of` binding) borrows
     the element. A holder that stores the read acquires a count
     (§70 rule 2). A discarded read changes no count.
   - A method that returns its receiver (`fill`, `reverse`,
     `copyWithin`) gives a borrowed array reference. A holder that
     stores it increments the holder count. A discarded result changes
     no count.
   - `length` reads no element.
   - An operation that reads the elements and keeps no reference to
     the array borrows the array and changes no holder count.
     `Promise.all(a)` is one: its aggregate takes its own count of
     each input (§166 rule 3).

   These are all the operations on a dynamic array of a counted type:
   the array literal, `Array.of`, `Array.from`, the index read and
   store, `length`, `for…of`, a spread, `push`, `pop`, `at`, `slice`,
   `concat`, `fill`, `reverse`, `splice`, `shift`, `unshift`,
   `copyWithin`, `Promise.all`, and a `yield` of the array (rule 5b). An accepted form that the checker rewrites to one of
   these operations (for example, `Array.from` to a spread literal)
   has the class of that operation.
5a. **No callback method carries a counted value.** The checker
   rejects, with S014, an array method or a collection function that
   takes a callback (the callback family, `sort`, `flatMap`,
   `Map.groupBy`) if a counted type is in any of
   its positions: the receiver element, a callback parameter, the
   callback result, the accumulator, or the method result. This holds
   at every depth, on a dynamic array and on a `FixedArray`. The
   checker also rejects the equality searches and `join` if the
   receiver element is counted. A handle element is already rejected;
   this extends the same rejection. Measured at `96f46f35`: on
   `Promise<void>[][]`, the checker accepts `find`, `filter`, `map`,
   `some`, `sort`, `flatMap`, `includes`, and `indexOf`; on
   `Promise<void>[]` it rejects each with S014. Measured at `96f46f35`:
   `map`, `flatMap`, `reduce`, and `reduceRight` on an `i32[]` or a
   `FixedArray<i32, 1>` accept a callback that returns
   `Promise<i32>[]` (`map` keeps one count; the dynamic reductions fail
   the LIR verifier). The rejection uses the existing S014 element-kind
   sites, and their witnesses in the §154 table name a nested counted
   element and a counted callback result. The method table derives
   each method's class from rule 5 or 5a; a method with no class is a
   build failure.
5b. **A yield is a copy site.** `yield v` of a counted type stores `v`
   in the result of `next()`, as a `return` stores its value (§70
   rule 2a): the store acquires a count, and a fresh owner transfers
   its count. An `IterResult<Y>` of a counted `Y` holds its `value` as
   a `FixedArray` holds an element (rule 6): a copy acquires, the exit
   of its holder releases, and a read of `value` borrows. A `for…of`
   over a generator follows the same rules for its binding (§70 rule
   2, `ForOfBinding`). Measured at `96f46f35`: a generator that yields
   a handle (`Generator<Promise<i32>>`) or a handle array, and drops
   its own local at normal completion, makes the caller's later
   `await first.value` fail in each tier (dev JIT and C AOT: `async
   resume without completion`; interpreter: `unknown packed async
   handle`). Node prints the value.
6. **A `FixedArray` holds its elements by value.** A copy of a
   `FixedArray` of a counted type acquires each element. The exit of
   its holder releases each element. An element store follows rule 5.
7. **One release operation per counted type.** The release of a counted
   value is derived from its static type: a handle releases its frame
   (§70), a dynamic array decrements its holder count (rule 4), and a
   `FixedArray` releases each element. The runtime release of an array
   element receives that derived description. It never infers the
   element kind from the element bytes.
8. **A completion owns its counted value.** The registration of an async
   frame records the release of its fulfilled type (rule 7), or records
   that the type is uncounted. When the frame is freed, the runtime
   releases a cached counted value. Each `await` result of a counted
   type acquires its own count before the next statement, and the
   result is a fresh owner that one store or one discard consumes
   (§70 rule 4: an await does not consume the ownership). A discarded
   `await` result of a counted type is released at the statement. The
   §166 restriction on a counted aggregate result does not change.
9. **A total check.** The LIR verifier reports, by instruction, each
   element operation of rule 5 on a counted array and each counted
   completion read of rule 8 that does not have its count operation, at
   every depth: an element store without its acquire or transfer, a
   replaced element without its release, a removal whose result is not
   consumed, and an await result of a counted type that is not
   acquired. A unit test builds each violating LIR form by hand and
   reads the message (core principle 9). An operation whose count
   actions run inside one runtime or emitted operation (a bulk
   mutation, a copy of an array, a spread, a completion read) carries
   its count action in its LIR instruction: the release description of
   the element or result type (rule 7), or the fact that the type is
   uncounted. The verifier derives the description from the operand
   type and compares the two. The unit tests of each runtime operation
   cover the actions inside it. The copy and the exit of a
   lexical holder stay on the one lowering path of §70 rule 2b. The LIR
   does not carry a lexical holder or its scope exit, so the verifier
   cannot check them; round 3 measured that a lowering which omits both
   count operations of a local alias passes the verifier (171.3 item 2).
10. **The release can trap.** Each release in rules 4 to 8 that frees a
   frame with an unobserved exception traps with §116.1 rule 4, at the
   position of the operation that releases it. Each LIR instruction
   that can release a counted value (its count action is counted, or it
   releases an operand) carries a trap site, in each tier. The verifier
   reports an instruction with a counted release and no trap site.
11. **A temporary is released on one path.** A fresh counted operand of
   any call (a receiver, an argument, a builtin method such as `pop` or
   `push`, an array method, an intrinsic) that the call does not store
   is released after the call, on one lowering path for every call
   kind. The verifier requires that each fresh counted owner, including
   a receiver, is consumed: stored, transferred, or released.
12. The three tiers (dev JIT, C AOT, interpreter) give the same counts,
   the same trap, and the same output. The interpreter has the same
   holder count and element ownership.
13. A generator that is suspended and then dropped keeps its locals.
   This section does not change that (171.3).

### 171.2 Acceptance

1. Red first, at the contract pin. Accept entry `a338`: each mutation of
   rule 5 on one array and through each alias form of the measurement
   (a synchronous parameter, an async parameter, `const b = a`, a module
   global, a return value, a nested element), a live local handle kept
   across `copyWithin` and an aliased `push`, a nested array, a
   `FixedArray`, and an async function that returns a handle and a
   handle array, awaited twice. The program prints a value after each
   await, so a freed and reused frame changes the output. Accept entry
   `a339`: the rule 5b probes (a yielded handle, handle array, nested
   array, and `FixedArray`, each awaited after the generator
   completes, and a `for…of` over the generator). Accept entry `a340`:
   a `for…of` over a module-global handle array whose body resets the
   global (rule 5 hold). Trap entries,
   each a failed task that the program never observes: `t87` (removed
   by `shift` and discarded), `t88` (removed by `pop` through a
   synchronous parameter), `t89` (held in the handle array that an
   async function returns; the holder of the awaited result exits),
   `t90` (held in a `Promise<void>[][]` local that exits), `t92` (a
   failed handle yielded by a generator that completes; the caller's
   `IterResult` holder exits), `t93` (a counted `fill` that replaces
   a failed unobserved task; rule 10), `t91` (held
   in a `FixedArray` local that exits). Reject entry `r387`: `find`,
   `sort`, and `includes` on a `Promise<void>[][]`, and `map` and
   `reduce` on an `i32[]` with a counted callback result (rule 5a),
   with the
   §154 divergence that the handle-element S014 rejection carries (no
   `collisions.md` row names the element-kind rejection). Each with
   its measured `tsc` header and its pin result.
2. Unit tests: the four count-form tests of §170
   (`codegen/tests/task_group_count_form.rs`,
   `codegen/tests/task_group_array_count_form.rs`,
   `runtime/tests/counted_completion_form.rs`,
   `runtime/tests/task_group_array_count_form.rs`) change to state that
   no task stays live. A three-tier test runs each row of the
   measurement tables 1 (the array, `FixedArray`, and completion rows),
   2, and 3, with its zero control, and requires zero retained tasks.
   A runtime test of the holder count and of the free at zero. A layout
   test of the 40-byte payload, and a C `offsetof` check of each field.
3. The async-cost benchmark: the median is at most 1.05 times the pin
   median (release, best of three). The standing performance gate
   passes.
4. The LIR text golden `codegen/tests/lir-goldens/corpus.txt` moves for
   the new entries and for the count instructions. No other `.expected`
   golden moves.

### 171.3 Open

1. A suspended generator that is dropped keeps its locals and their
   counts (measurement item 1, generator row). A dropped iterator has
   no scope exit.
2. The LIR does not carry a lexical holder or its scope exit
   (`specs/tracking/s171-counted-holders.md` item 10). A form with a
   holder identity and an exit boundary lets the verifier check the
   copies and exits of rule 3. Lexical exits had no measured defect.
3. A lambda that captures a counted local borrows it (§116.1 rule 4c),
   but the checker accepts the lambda in a `let` of an outer block, and
   a call after the captured block exits reads a released value.
   Measured at `96f46f35` for a captured `Promise<i32>`,
   `Promise<i32>[]`, and `IterResult<Promise<i32>[]>`: the dev JIT and
   the C AOT tier fail at the later await, and node prints the value
   (`specs/tracking/s171-counted-holders.md` item 14). The defect is a
   capture lifetime, not an array operation; a later section closes it.
4. `await G[0]` borrows the element of a module-global array. If
   another task resets `G` during the suspension, the awaited frame can
   be freed. This was present before §171, through the handle-array
   release (Phase Review 2; not measured).
