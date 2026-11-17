<!-- §323 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 323. A foreign call keeps the storage it borrows

*(Added 2026-11-17.)* Origin: the array borrow round of the memory
safety study (`specs/tracking/memory-safety-study.md`, MS3-3). The owner
told the work to fix it on 2026-11-17.

Problem, measured at `c19d3dd8` in the ship tier. A foreign C function
`vhReenter(vertices, info)` reads `vertices.data[0].x`, fires the
callback of `info`, and reads `vertices.data[0].x` again:

```ts
function work(): f32 {
  const local: Vertex[] = [];
  local.push(new Vertex(1, 2, 3, 4, 5, 6));
  const info = new VhCallbackInfo((message, u1, u2) => {
    Context.collect();
    /* allocate eight arrays of four vertices with x = 9 */
  }, null, null);
  return vhReenter(local, info);
}
```

The C function reads `1`, then `9`. The emitted C clears the shadow root
of `local` before the call (`roots.v0 = 0;`), because the call uses only
the data pointer and the length. The collect in the callback releases
the storage while C still reads it, and a later array takes the block.

A second form: the callback pushes past the capacity of the array that
the call borrows. The runtime moves the data block, and C reads the old
block (measured: `1`, then `0`, no error). Collisions.md Q4/Q15 states
that "the callee borrows, the caller retains ownership"; both forms break
it.

### 323.1 Rules

1. A value whose storage a foreign-call argument takes (the data and
   length of an array, the bytes of a string, an array pair in a
   struct member, any other argument that the crossing plan derives from
   a Context allocation) stays a root from the point where the argument
   is derived until the foreign call returns. A collect during the call,
   from a callback at any depth, keeps that storage. This holds in each
   tier.
2. While a foreign call holds the storage of an array, a reallocation of
   that storage (a push or another growth past the capacity) gives a
   runtime error with a trap kind of its own and a message that names
   the cause: the array storage is borrowed by a native call. An element
   write, a `pop`, and a push within the capacity do not give the error.
   C sees an element write at once (zero copy). Foreign calls nest; the
   borrow ends exactly when the call that started it returns.
3. LIR carries the link from each derived argument to the foreign call
   that uses it (core principle 8). The root of rule 1 and the borrow of
   rule 2 come from that link in each tier, not from a map in one
   emitter. `ForeignArrayData` and the other derivations name the call
   or the call names them; the form is the implementation's choice, and
   the LIR verifier rejects a derivation whose call does not name it.
4. The borrow costs a constant amount per array argument of a foreign
   call: no cost per element, and no cost that grows with the array
   size. The memory study measured +0.3 ns per call in the ship tier.
   A field that the borrow needs uses the padding of `ArrayHeader` at
   offset 36; the header size stays 40 bytes.
5. No accepted program changes its output, except a program that reaches
   the states of the problem above, whose output was a wrong value. Each
   tier that runs a foreign call (the development tier and the ship tier;
   the interpreter does not call native code) gives the same output and
   the same runtime error.

### 323.2 Acceptance

1. Red first: an interop accept entry with the program above, the
   string form if the measurement of acceptance 4 shows that it reaches
   the defect, and a callback that writes an element and pushes within
   the capacity during the call. Its output differs at the pin. The
   native fixture function goes into `corpus/interop`, and its mirror is
   generated with `subscript bind`, not edited by hand.
2. A callback pushes past the capacity during the call: a trap entry
   if the trap corpus can call native code, else a test in the form of
   the existing native-call tests. The development tier and the ship
   tier give the same kind, message, and position.
3. A test that a nested call keeps the borrow of the outer call, and a
   test that the borrow ends after the call (a push past the capacity
   after the call succeeds).
4. The report names each argument kind of rule 1 (array, string, member
   array pair, other), with its result at the pin and after the change,
   measured.
5. The CLI `check` output of each corpus entry is the same at the pin
   and after the change. LIR text goldens move only for entries with a
   foreign call.
6. The `bound-call` benchmark keeps its time within its spread, or the
   report states the measured difference.

### 323.3 Phase Review

Form: a foreign `Call` and a foreign `HostCompletion` take the managed
array operand, and each tier derives the pointer and the length inside
the call. `ForeignArrayData` is removed. A derived field or element
address keeps its owner as a dependency in `value_liveness`, also
across a suspension. The runtime keeps a stack of native-call borrows
per Context. The collector reads it as roots. Each array borrow saves
and restores the count at `ArrayHeader` offset 36. `array_push` gives
the trap kind `array-storage-borrowed` (36) on a growth while the count
is not zero.

Measured with a473 (100 × the first read + the read after the
callback):

| Kind | Pin, ship tier | Development tier and ship tier |
|---|---|---|
| Local array, collect in the callback | 109 | 101 |
| Member array pair, replaced before the collect | 109 | 101 |
| Pointer into a Context object | 224 | 101 |
| Member string, replaced before the collect | 6590 | 6565 |
| String argument | 6565 | 6565 |

Cost: the `bound-call` benchmark (one array call and one integer call
per pair) went from 5.417 ns to 8.875 ns per pair. The difference is
outside its spread. The cost does not change with the array size. Each
foreign call with a derived argument calls the runtime for the scope
start, for each borrow, and for the scope release. This includes a call
that passes only a string or a struct pointer.

The first full gate failed on three tests that read the former call
form: a verifier unit test and two `cli/tests/boundary.rs` tests that
read the emitted C. The coding agent changed the tests to the new form.
The emitted C was correct. The next full gate passed.

One fresh review found no CRITICAL or MAJOR finding. One MINOR finding
is fixed: a test assertion that checked only for the word "call". Two
MINOR findings are open or need no change: a `Context.free` in a callback of the object that a borrowed pointer names is outside rule 1 by design, and the cost note is 323.4 item 1.

A separate read of the collector windows at this pin found that the
dependency rule of `value_liveness` also closes a stale owner of a field
address (a method of a `@ValueType` field of a temporary object that
runs a collect). No corpus entry covers that form yet.

### 323.4 Open

1. The scope cost of a foreign call that passes no array (a string or a
   struct pointer) is not measured. A form that saves the count inline
   for a top-level array argument (the study measured +0.3 ns) was not
   built.
2. Contrived, MINOR: a `sort` comparator that replaces elements that it
   does not compare, then collects. The runtime keeps those elements
   only in its scratch copy, which is not a root, and writes them back.
   Measured with four elements: the development tier and the ship tier
   print `3 4 9 9`; the interpreter and `node` print `1 2 3 4`.
3. The form of the field-address owner (a method of a `@ValueType`
   field of a temporary object that collects) has no corpus entry. The
   pin printed `9` in the development tier and the ship tier; this
   change prints `1` in each tier.
4. The host tutorial does not state that an array pointer passed to a
   completion function ends when that function returns.
