<!-- §325 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 325. A C write that replaces a pointer is received or stops

*(Added 2026-11-17.)* Origin: §187.3 items 1, 4, and 8, rank 2 and
rank 4 of `specs/tracking/backlog.md`. The owner selected it on
2026-11-17 and decided the form: a replaced pair becomes a new script
array, and a replaced pointer member, userdata slot, or callback field
gives a runtime error.

Problem, measured at `7693e340` in both tiers. A typical C API returns
an array through an out-struct (`void getItems(Out *out)` sets
`out->items` to a C array and `out->itemsCount` to 3). The script reads
its old array, with no diagnostic:

| C writes | Script reads after the call |
|---|---|
| count-first pair `size_t itemsCount; const Item *items;`, replaced by 3 C items (script array had 2) | the old 2 items |
| the same pair behind a pointer member | the old 2 items |
| the same pair, script array empty | 0 items |
| pointer-first pair `const Item *items; size_t count;` in an out-struct | `bind` rejects the function (no read lowering) |
| pointer member `Scene.mesh`, replaced by another C `Mesh` | the old mesh |
| userdata slot, replaced by a non-null C address | the old value |
| callback field, replaced by another C function | the old callback |
| a pair that C does not replace; C writes its 3 elements | the new values (zero copy holds) |

§187 rules 4, 7, 8, and 11 state that these writes are lost. A struct
whose bytes the call passes as script memory has no compare at all, so
there a C write of a pointer member becomes a script reference to C
memory (§187.3 item 8, measured `3 7`).

### 325.1 Rules

1. **A replaced pair is received.** In a read position (§187 rule 1),
   after the call, each tier compares the pointer and the count of each
   pair (count-first and pointer-first, at every reach of §187 rule 11)
   with the values that the call gave C. If C changed either one, the
   script member gets a new script array that holds a copy of the
   `count` elements at C's pointer. The C memory stays C's. If C changed
   neither, the script array stays, and C's element writes stay visible
   (zero copy).
2. **The copy reads each element by its read lowering.** Scalars,
   enums, and structs that copy their bytes copy as bytes. A struct
   element that the call builds as a scratch struct converts by the
   §187 copy-back, a string view included (§187 rule 2). A handle
   element copies its value. An element type with no read lowering
   keeps the §187 rule 2 rejection at check time. A received element
   compares each pointer member, `void *` userdata slot, and callback
   field (at any depth, in both crossing forms) by rule 4. If C kept
   the pair pointer and changed only the count, an element at an index
   below the earlier count compares with the values that the call gave
   C for that element. Every other received element has no script
   value before the call, so it compares with null: null becomes null
   when the type allows null, and any other value gives the rule 4
   runtime error. If C kept the pointer and wrote a count above the
   earlier count, the call gives the rule 3 runtime error. The error
   gives the element index in the member path.
3. **A bad pair stops.** A null pointer with a count of 0 gives an empty
   array. A null pointer with a count above 0, or a count above the
   largest `i32`, gives a runtime error.
4. **A replaced pointer stops.** After the call, each tier compares each
   C-writable pointer member, `void *` userdata slot, and callback field
   with the value that the call gave C. If C stored a different non-null
   value, the call gives a runtime error. If C stored null, the script
   member becomes null when its type allows null, and the call gives the
   runtime error when it does not. A C pointer never becomes a script
   reference.
5. **Both crossing forms.** Rules 1 and 4 hold for a struct that the
   call builds as a scratch struct and for a struct whose bytes the call
   passes as script memory. For script memory, the call keeps the values
   of rule 4 before the call, and compares after it.
6. **The error names the cause.** The runtime error has a trap kind of
   its own. The message names the C function and the member path, and
   says that C replaced a pointer that the script cannot receive. The
   position is the call site. Both tiers give the same kind, message,
   and position.
7. **`const` is not compared.** A member that C cannot write (behind a
   `const` pointer, §187 rule 7) gets no compare.
8. **One plan.** The crossing plan of §187 rule 3 names each pair
   read-back and each compare of rule 4. Both tiers, the binder, and the
   checker read it. `bind` stops rejecting a pointer-first pair in an
   out-struct for the lack of a read lowering. The mirror spells each
   pair as `T[]`, so the mirror records the C order of each pair in a
   struct: `bind` writes a `@subscript-c-member` record for each pair
   (scalar elements included), with the key `order=count-first` or
   `order=pointer-first`. The count is `size_t` in both orders. The
   checker carries the order to the crossing plan. A pair with no
   record, or with no `order` key, counts as count-first. The two halves
   of a pair stay adjacent.
9. **Cost.** A call that replaces nothing pays one compare per member
   of rules 1 and 4. A replaced pair pays a copy that grows with the
   count that C wrote. That cost comes from the data that C returns, so
   core principle 16 does not reject it.

### 325.2 Acceptance

1. Red first: an interop accept entry, with the fixture functions in
   `corpus/interop` and the mirror from `subscript bind`. It covers: a
   count-first pair that C replaces (script array non-empty and empty),
   the same pair behind a pointer member, a pointer-first pair that C
   replaces, a pair of structs with a string view that C replaces, a
   pair that C does not replace but whose elements C writes, a pair
   that C sets to null with count 0, and a pointer member that C sets
   to null. Each line prints what the script reads. At the pin, the
   entry fails in both tiers.
2. A runtime error test for each of: a replaced pointer member, a
   replaced userdata slot, a replaced callback field, a pointer member
   of a struct passed as script memory, a null pair with a count
   above 0, and a received element with a non-null pointer member.
   The development tier and the ship tier give the same kind, message,
   and position.
3. A `const` pointer target that C does not write gets no compare: the
   emitted C of a read-only call has no compare code for it.
4. The `bound-call` benchmark, and a call with one out-struct that C
   does not change, measured at the pin and after the change. The
   report states the difference.
5. The CLI `check` output of each corpus entry is the same at the pin
   and after the change, except for the functions that `bind` rejected
   before rule 8. LIR text goldens move only for entries with a foreign
   call.

### 325.3 Sections this one amends

- §187 rules 4, 7, 8, and 11: a replaced pair is received; a replaced
  pointer, userdata slot, or callback field stops. §187.3 items 1, 4,
  and 8 close.

### 325.4 Phase Review

Form. The crossing plan names a pair read-back (`WriteBack::Pair`,
`ReceivePlan`) and the compares of rule 4 (`WriteBack::Pointer`, and
`WatchPlan` for script memory). The mirror records the C order of each
pair in a struct (`order=` on `@subscript-c-member`). The checker
carries it through HIR and LIR. Both tiers emit a finite watch inline:
one load before the call and one compare after it for each slot. A pair
whose elements hold watched slots takes one buffer that the Context
reuses, then compares in a loop. A recursive plan (an embedded header
that reaches its own type) keeps a runtime walk. The trap kind is 37,
`ForeignPointerReplaced`: `<function>.<path>: C replaced a pointer that
the script cannot receive`, or `C returned an invalid pointer/count
pair`.

Measured with a475 and the interop fixtures, development tier and ship
tier the same:

| Form | Pin | After |
|---|---|---|
| Count-first pair replaced, script array non-empty (length, first) | `2 1` | `3 11` |
| The same, script array empty (length) | `0` | `3` |
| The same pair behind a pointer member (length) | `2` | `3` |
| Pointer-first pair replaced | rejected by `bind` | `3 11` |
| Pair of structs with a string view replaced | length `0` | `first second 41 42` |
| Pair kept, C writes the elements | `11 12 13` | `11 12 13` |
| Pair set to null with count 0 (length) | `2` | `0` |
| Pointer member set to null (`=== null`) | `false` | `true` |

Each error of acceptance 2, a count above the earlier count with the
same pointer, and an element pointer replaced at index 6 of eight give
the same kind, message, and position in both tiers.

Cost, release, ns per call, pin then after (development tier, ship
tier): `World { Child *child; }` with a watched pointer, C writes
nothing: 11.12 to 10.53, 9.55 to 9.88. One unchanged out-struct: 15.74
to 14.17, 12.82 to 11.45. A pair of eight structs that each hold a
pointer member: 15.17 to 28.43, 12.70 to 18.97. A receive of 1,000
struct elements: 21.5 µs and 18.8 µs (the pin does not receive). The
first form of the watch (a runtime graph walk per call) measured 56.8
and 59.1 ns for `World`; the inline form replaced it.

The first full gate failed on 11 tests in targets that the coding agent
did not run (tests of the removed §187 userdata rejection, the old
"lost" result, the committed mirrors, a tutorial excerpt, and a parser
assertion that a mutated mirror reached). Each now tests the §325
result; the parser case is avoided in the checker. The reject entry
r413 (a userdata slot that C can write) is deleted: rule 4 compares the
slot now, and a trap test covers its embedded external-mirror shape.

Two fresh reviews. The first found two MAJOR findings, both realistic:
`bind` changed the by-value parameter of a two-field descriptor from
`T[]` to the struct, and the watch did heap work on each call. Both are
fixed. It also found seven MINOR findings, fixed: a check that could
not fail, a third C layout function with a silent fallback, one
predicate written five times, watch plans missing from the `boundary`
output, the coverage of r413, and the order record parse. The second
review found no CRITICAL or MAJOR finding. Its three MINOR findings are
fixed: a "capacity in, count out" call stopped with a false error (rule
2 now compares a kept element with its earlier value), the receive
built path text on the success path, and a descriptor in a fixed array
field reached an internal error.

### 325.5 Open

1. A script callback that runs during the call and stores a different
   reference into a watched slot of script memory gives the rule 4
   error. The compare cannot tell a script write from a C write. Rare.
2. A recursive watch plan (an embedded header that reaches its own
   type) keeps a runtime walk with a visited set. Its cost is not
   measured.
3. Contrived: a count above the largest `i32` has only a runtime unit
   test, not a test in both tiers.
4. The development tier pays 13 ns for the eight-element pair, and the
   ship tier pays 6 ns. Earlier runs gave 27 to 35 ns. The cause of
   the spread is not known.
5. The `bound-call` benchmark exits 2 on its decomposition check at
   the pin and after, so it gives no valid comparison here.
   *(Closed 2026-11-19 by §340 rule 2.)*

