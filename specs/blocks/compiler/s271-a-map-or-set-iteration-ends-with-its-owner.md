<!-- §271 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 271. A Map or Set iteration ends with its owner

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R12.
The owner selected the form on 2026-11-13: the iteration state is a
counted value that the release owners of the lowering hold.

Problem, read at `09d99a57`. A traversal of a `Map` or a `Set` calls
`subscript_rt_assoc_iter_begin`, which adds 1 to the iteration depth of
the container and returns the bound of the visit
(`runtime/src/assocops.rs` `iteration_begin`). While the depth is not 0,
the container does not compact its entry storage, because a compaction
moves the entries of a visit that is open. `forEach` and `Array.from`
call `subscript_rt_assoc_iter_end` at the end. A `for…of` over a `Set`,
or over `keys()` or `values()` of a `Map`, calls `begin` and never calls
`end`, in the development tier,
the ship tier, and the interpreter. So after one such traversal, the
depth of the container stays above 0. Each later delete and insert
appends to the entry storage, which never compacts. A long-lived `Map`
that a host loop traverses and changes grows without a bound, while
its size stays small.

### 271.1 Rules

1. An open iteration of a `Map` or a `Set` has an owner: the iterator
   cursor of rule 7. A `for…of` makes one cursor at the loop entry and
   holds it in a local of the loop. `Map.forEach` makes two cursors and
   `Set.forEach` makes one. The language has no iterator object that a
   script can keep (S014). The storage of a `Map` or a `Set` is owned by
   the Context, so the cursor holds no count of the container.
2. The release of the owner ends the iteration: it subtracts 1 from the
   depth of the container, if the container is live.
3. The release owners of the lowering release the owner on each exit
   of its scope: the end of the loop, `break`, `return`, an exception
   edge, and the release of a generator or async frame that holds it.
   The language has no labeled loop, so no `continue` leaves the loop. No new list of exit sites is made: the
   owner is a counted local, and the present release rules (§255) cover
   it.
4. A visit keeps the present semantics, which `node` gives too: a
   removed entry is skipped, and an entry added during the visit is
   visited. The coding agent measured `insert:4` in `node` and in the
   three tiers at the pin.
5. The three tiers give the same output. The memory accounting finds no
   owner that stays live after its scope, and no count released twice.
6. §255 rule 4 is extended: the release model of the LIR test counts the
   cursors of rule 1 as lexical owners: one for each `for…of` over a
   `Map` or a `Set`, two for `Map.forEach`, and one for `Set.forEach`. The model
   reads the kind of the loop from the HIR, not from the lowering.
7. The owner is the iterator cursor of the loop (`ValueType::Iterator`
   of a `Map` or a `Set`), with a count action. The release owners of
   the lowering hold a `ValueType`, not only a data type, so a cursor
   is a lexical owner as a counted data value is. The release of the
   cursor runs the release action of rule 2. The cursor needs no
   allocation. An advance of the cursor changes its position and is
   not a new owner, and a copy of the cursor that a generator or an
   async frame saves is the same owner, so no count is released twice.
   The coding agent measured three forms. A new internal variant of
   `Type` changes 22 sites in 12 files, the checker and the HIR among
   them, and allocates once for each loop entry. The counted cursor
   changes about 16 sites in 11 files of the LIR and the three tiers,
   and allocates nothing. No third form was smaller.
8. An async frame that is not complete is not dropped when its last
   handle is released (`runtime/src/context/async_scheduler.rs`). So an
   iteration in such a frame ends when the frame completes, or when the
   Context drops, and not at the release of the handle.

### 271.2 Acceptance

1. Red first, at the pin: a test traverses a `Map` with `for…of`, then
   runs 100,000 pairs of a delete and an insert with 4 live entries,
   and reads the entry capacity of the container (a runtime test hook,
   or the reserved bytes of the memory accounting). At the pin the
   capacity grows with the number of inserts. The tracking note records
   it.
2. After the change, the capacity stays at most a small constant.
3. The same test for each exit of rule 3: `break`, `return`, a throw
   that a `try` catches, and a `for…of` in a generator that the caller
   drops after one `next()`.
4. A control: the accept entry
   `a444-a-map-or-set-iteration-ends-with-its-owner` holds each exit of
   rule 3, a delete during a visit that skips the entry, and an insert
   during a visit that the visit reaches. It is `js-comparable`, and the
   three tiers and `node` give the same output.
5. The release model of the LIR test accepts the LIR under rule 6. A
   unit test of the model builds the violating form: the LIR of a
   `for…of` over a `Map` with the release of the owner on the
   exception edge removed. The model reports it. A control: the same
   LIR with the release passes. Before rule 6, the coding agent
   measured that the model reports one release more than the lexical
   scopes require on the exception edge.
6. A loop over an array, and a loop over a `Map` or a `Set`, allocate
   nothing more than at the pin. A loop over an array does not change.
7. The LIR text snapshot changes only in the blocks of the async
   entries that loop over a `Map` or a `Set`: `a146`, `a224`, `a341`,
   `a427`, and `a432` (12 cursors), and grows by the block of `a444`.
   The release of a cursor on each exception edge adds blocks, so the
   branches and the value numbers of those blocks change too. The coding
   agent measured +1,270 and −650 lines. The LIR verifier and the release
   model accept each block, and no output changes. No other golden
   moves; if one does, report and stop.

### 271.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review ran each exit form in the development tier
and the ship tier: a plain loop, `continue`, a `return` from two nested
loops, a throw through two loops, an async throw after an `await`, a
generator run to its end, a generator dropped after `next()`, a lambda
that returns from a loop, and a throwing callback of `forEach`. A test
with the visit left open took more than 30 seconds for 400,000 delete
and insert pairs, and each form above took 0.4 seconds, so each form
ends its visit. A loop that deletes and inserts in its body, with a
nested loop, a `return`, a throw, a generator, a reassignment of the
`Map`, and `clear()` inside, prints what `node` prints, so no form ends
a visit twice. Each advance of a cursor now reads the bound, where the
pin added 1 to the depth at each advance.

1. MINOR, realistic, fixed: rules 1, 2, and 6 stated a count of the
   container that the cursor does not hold, and did not name the
   cursors of `forEach`.
2. MINOR, contrived, fixed: the tiers lower a retain of a cursor as a
   copy, and the LIR verifier accepted one. The verifier now rejects a
   retain of a cursor, so no lowering can end a visit twice that way.
3. MINOR, contrived, recorded: §271.4 item 1.
4. MINOR, contrived, recorded: §271.4 item 2.
5. MINOR, fixed: the tracking note held a claim that a later part
   contradicted.
6. MINOR, recorded: the growth test runs 2,000 pairs, not the 100,000 of
   acceptance item 1, at a cost of 0.04 seconds. Acceptance item 6 rests
   on the source: the cursor is a value of the frame.

The review found a program with no `Map` that does not end in the
development tier and the ship tier: an outer loop, a loop over an array
with `break`, and a loop over a generator with an `await` in an async
`main`. A binary built at the pin of this section gives the same
result, so this section did not cause it. §271.4 item 3.

### 271.4 Open

1. `iteration_end` reads only whether the address of the container is
   live. A script that frees a `Map` with `Context.free` inside its own
   loop, and a new `Map` at the same address with a visit open when the
   old cursor is released, takes 1 from the depth of the new `Map`.
2. The release of a generator frame in the interpreter ends each
   iterator value it holds, and does not read the kind of the cursor.
   Only a `Map` or `Set` cursor reaches it now.
3. This program does not end in the development tier or the ship tier
   (`node` prints `end 2`):
   `function* gen(): Generator<i32> { yield 1; yield 2; }` and an async
   `main` with `for (let r = 0; r < 1; r++) { for (const j of [0, 1])
   { if (j > 0) { break; } } for (const x of gen()) { await
   Context.suspend(); n++; } }`. Each of the four parts is necessary.
