# Memory safety study: versioned references and array borrows

Status: **STOPPED 2026-11-17 by the owner.** Discussion and measurement
only. No contract changed, and no production code changed. Each
prototype was built in a scratch copy of the pin `c19d3dd8` outside the
repository. This note records the decisions, the numbers, and the open
questions, so that a later study can start from them.

## Question and owner decisions

A proposal (not in the repository) asked for versioned references in
place of payload addresses, exact tracing, and explicit memory budgets.
The owner decided these points on 2026-11-17:

1. A stale handle (a handle used after its allocation is released) and a
   second release of one handle are defects without conditions. This
   decision is the premise of this study only. It does not change Q6
   ("undefined in both tiers") in `specs/blocks/collisions.md`: a
   `Context.free` at a wrong point is still in the design, and a fix
   outside this study uses the severity rule of invariant 6.
2. Keep `Context.free` and use versioned references. The other option,
   to remove `Context.free` and keep only `Context.collect`, was not
   chosen.
3. The host boundary is in scope: a handle that the host passes back
   after a release must give a runtime error. The safety of host C code
   is out of scope.
4. Zero copy of large data (vertex arrays and other value arrays) must
   stay. Persistent mapping (the host keeps an array pointer after the
   call) is a realistic use.

## MS1-1: inventory at `c19d3dd8`

- A reference is the payload address (8 bytes) in each tier. The ship
  Context uses a size-class arena with a LIFO free list, so the next
  allocation of the same size takes a released address.
- With the default settings, a stale reference reads and writes the new
  object in each tier after reuse: an alias, an array element, a closure
  capture, a reference across `await`, a Map, and callback userdata. A
  second `Context.free` after reuse releases the new object.
- Without reuse, the interpreter and the development tier stop with a
  runtime error. The development-tier check reads the header of a block
  that the system allocator already released
  (`codegen/src/lower/func/value.rs`). The ship tier gives wrong values.
- The C emitter removes each `DevOnlyLifetime` and `DevOnlyRelease` site
  (`codegen/src/cemit/graph.rs`). With freed-handle diagnostics on, a
  ship-tier field read after a release returns the old value with no
  error.
- If the host releases the Context and then fires a callback, the ship
  tier stops with signal 11 (3 of 3 runs). The development tier stops
  with a runtime panic.
- `Context.free` accepts only `object` types: classes, Map, Set, and
  Error (`compiler/src/ambient.rs`). An array, a string, or a function
  ends only by `collect` (unreachable) or by counted release at the last
  holder. So only references of the four free-able kinds need a version
  check.

## MS3-1: P8, an 8-byte versioned handle (ship tier only)

Form: handle = `(generation << 32) | slot`. Each Context has a slot table
with a free list. Each field access, receiver, and runtime call on a
free-able kind resolves the handle first: a bounds compare, a generation
compare, then a payload load. Arrays and strings stay raw pointers.

| Measurement | Result |
|---|---|
| MS1-1 stale-handle programs (16) | each gives a runtime error before any wrong value |
| Host callback after release and reuse | runtime error (`c19d3dd8` printed the wrong object) |
| Host callback after Context release | signal 11, unchanged (P8 holds no Context identity) |
| Accept corpus (ship tier) | 359 equal; 3 worker entries fail (message copy not adapted) |
| Time, prototype / pin | tree 1.202, collect 1.042, particles 1.000, callbacks 1.001, fib-recursive 1.001, sort 1.001 |
| tree, +19 ms | compares 5.3 ms; table loads and slot work in the runtime 13.5 ms |
| Code size | tree program text 1,108 to 1,372 bytes; +0 bytes without free-able kinds |
| Slot table | follows the peak live count (churn of 1,000,000 with 1,000 live: 1,002 entries); never shrinks (tree keeps 2 MiB) |

Each resolution adds 4 loads and 2 compare-and-branch pairs. Clang
merges two resolutions of one handle when no store comes between them.

## MS3-2: generation policy at collect

- C1: a collect does not advance the generation of a slot it releases.
  No handle to that slot exists in scanned memory. A slot whose handle
  went to the host (callback userdata, registration) is marked
  "escaped" and keeps the advance.
- C2: during the mark phase, a bitmap records each slot that a scanned
  word names. After the sweep, each free or retired slot that is not
  named and not escaped gets generation 1 again.

Measured with a generation limit of 256 to make retirement fast:

| Case | P8 | C1 + C2 |
|---|---|---|
| free only, no collect | grows, 16 B per 255 releases | grows the same |
| free, collect every 1,000 cycles | grows to 4,923 entries | 1,005 entries, constant |
| collect only | 2,001 to 5,001 entries | 2,001 entries, constant |
| stale handle kept in a live object | runtime error | slot not renewed; runtime error |

The same programs with the bitmap ignored stop with signal 11, so the
check can fail. Time, C1 + C2 against P8: collect 1.007 (inside the
spread), churn3 1.094 (the renewal walk visits the whole table).

Remaining growth: a program that releases with `Context.free` and never
collects grows by 16 B per 2^32 releases on one slot. Invariant 2 forbids
a scan that nobody asks for. The recommended option was a budget of
retired slots that ends in an allocation failure that names the cause.
That option was not decided. C1 also makes soundness depend on complete
roots: a missed root lets a stale handle reach the next object in that
slot.

## MS3-3: array borrow and host pin

- B1: a foreign call saves the borrow count of each array argument,
  stores the count plus 1, and stores the saved count after the call. A
  reallocation of a borrowed array (a push past the capacity) gives a
  runtime error. The count uses padding at offset 36 of `ArrayHeader`.
  The size stays 40 bytes.
- B2: `subscript_rt_array_pin(ctx, data)` returns a versioned token, and
  `subscript_rt_array_unpin(ctx, token)` ends the pin. While a pin is
  open, a reallocation gives a runtime error, and a collect keeps the
  data block.

| Measurement | Result |
|---|---|
| Callback pushes past capacity during a foreign call | `c19d3dd8`: C reads a stale pointer (1, then 0), no error; prototype: runtime error |
| Persistent mapping, 100,000 vertices, 1,000 frames | 0 mismatches; the pointer does not move |
| Drop the array and collect while pinned | the host reads valid data; after unpin and collect, 3,145,728 bytes are released |
| Repeated or stale unpin | returns -1, no state change |
| Foreign call cost | +0.29 ns at 1 KiB, +0.32 ns at 16 MiB (independent of size) |
| Pin and unpin | 15 ns per pair; collect time does not change with 0, 10, or 1,000 open pins |
| Element access loops | particles 0.986, sort 1.000, push-heavy 0.997 |

Result: zero copy of large data stays, with no cost per element, and the
borrow lifetime can be checked at reallocation and at call edges.

The same round found a defect at `c19d3dd8` that does not depend on this
study. The emitted C clears the shadow root of a local array before a
foreign call. A `Context.collect()` in a callback during that call then
releases the array while C uses it (C read 9 in place of 1). A section
of the compiler contract fixes it (§323).

## Open questions at the stop

1. Retired slots in a program that never collects: a budget with an
   allocation failure, or accept the growth.
2. Pin tokens live in host memory, so C2 cannot renew them. One slot
   that pins 2^32 times grows the table by 16 B.
3. Where a pin is declared: the host API (prototyped), a script-side
   `Context.pin`, a retain declaration on the C function, or a
   fixed-capacity array type.
4. LIR does not carry the link from an array data snapshot to its call.
   The prototype used a map in the emitter. A production form needs a
   borrow start and end in LIR (core principle 8).
5. Context identity: P8 has no room for it. A host callback after a
   Context release needs a check that does not live in the Context.
6. The 16-byte handle (64-bit generation) was not measured. It changes
   layout, root words, element and key widths, and the boundary in all
   three tiers.
7. tree at 1.20: slot work in the runtime is not inlined, and a
   resolution is not reused across a store. Neither was measured.
