<!-- §254 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 254. Each `FixedArray` type has a C form

*(Added 2026-11-12.)* Origin: §220.4 item 2 and §218.4 item 2. Under
the owner's standing approval of 2026-11-06, selected by the owner on
2026-11-12.

Problem, measured at `cd3afcf0` with the debug CLI. `subscript run`
accepts each form and prints its line. `subscript build` fails the C
compile.

| Form | C compile |
|---|---|
| `const e: FixedArray<i32, 2>[] = [];` with no other use of the type | "use of undeclared identifier 'SubFA_i32_2'" |
| `const e: FixedArray<i32, 0>[] = [];` | "use of undeclared identifier 'SubFA_i32_0'" |
| `new Map<string, FixedArray<i32, 3>>()` with no other use of the type | "use of undeclared identifier 'SubFA_i32_3'" |
| `function mk(): FixedArray<i32, 0> { return []; }`, called | "initializer for aggregate with no elements requires explicit braces", twice |

Two causes:

1. The C emitter collects the aggregate types that get a `typedef` from
   the class fields, the globals, the function results, and the types
   of the values. A dynamic array or a `Map` holds its elements in the
   runtime, so its element type is not one of those. The allocation
   still names it in `sizeof`.
2. A `FixedArray` with the length `0` has no C form: C has no array
   with the length zero, and no struct with no members. The
   development tier lowers it, and §218.4 item 2 records that it stops
   the JIT when it is live across a suspension.

### 254.1 Rules

1. A `FixedArray` length is an integer literal from `1` to the maximum
   of `FixedArray`. The length `0` is rejected at the type with S100,
   with a divergence block: `tsc` accepts the type. The why states that
   C has no array with the length zero (invariant 1).
2. The C program has a `typedef` before the first use of each aggregate
   type that it names. The set of these types comes from each type that
   the emitter writes, so no use can name a type outside the set.
3. No accepted program with a `FixedArray` length of at least `1`
   changes its output. The development tier, the ship tier, and the
   interpreter give the same output.

### 254.2 Acceptance

1. Red first, at the pin: the accept entry
   `a438-each-fixedarray-type-has-a-c-form` with the first and third
   forms of the table fails the ship tier, and the reject entry
   `r500-a-fixedarray-length-is-at-least-one` with
   `FixedArray<i32, 0>` is accepted. The tracking note records the
   output. The header of `r500` states what `tsc` does, measured.
2. After the change `a438` passes the three tiers and equals its
   golden, and `r500` gives the S100 of rule 1.
3. A unit test pins rule 1 at a type annotation, a type argument, and a
   result type. A unit test of the emitter pins rule 2: a program whose
   only use of a `FixedArray` type is the element of an empty array or
   of a `Map` compiles.
4. The witnesses, the divergence entry, and the collision id move with
   the site. If a corpus entry or golden changes, report it and stop. No
   golden moves.

### 254.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review emitted each accept entry of the corpus, 328
entries, and found no aggregate name outside `program.c` or before its
`typedef`.

1. MINOR, contrived, fixed: the environment and frame structs named
   types after the set of rule 2 was read. Each such type was also
   named in a body, so no program reached the gap. The emitter now
   writes that text before it reads the set.
2. MINOR, recorded in the tracking note: the order of the typedefs
   follows the order of the names, and an aggregate that no C text names
   gets no typedef. The output of each program does not change.
3. MINOR, fixed: the emitter test repeated two forms of `a438`. It keeps
   the nested form only.
4. MINOR, fixed: the text of `collisions.md` Q3 and of the tracking
   note.
