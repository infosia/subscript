<!-- §298 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 298. The ship tier asserts the string view of a bound callback

*(Added 2026-11-15.)* Origin: §183.4 item 1. On 2026-11-15 the owner
told Claude to do each item of a list of open runtime defects, in an
order that Claude selects. This item is one of them.

Problem, read at `f01c0d15`. The binder admits one callback shape,
`void Callback(StringView message, void *userdata1, void *userdata2)`
(`bindgen/src/emit.rs`, `validate_callback_shapes`), where `StringView`
is the host string-view struct of the header. The runtime defines the
trampoline with its own string-view type. The ship tier
(`codegen/src/cemit/marshal.rs`) casts the address of the trampoline to
the host typedef, so the C compiler does not compare the two structs.
The binder compares the host view at bind time, but the callback record
in the HIR and the LIR keeps only the typedef name. So the emitted C
has no fact to assert, and the C file does not show that the host view
and the runtime view have one layout.

No good-faith header reaches a mismatch at `f01c0d15`: the binder rejects
another view shape. The defect is that the form does not carry the
fact, so a change of the binder or of the runtime view can make a
mismatch that no build finds (core principle 8).

### 298.1 Rules

1. The callback record of the HIR and of the LIR holds the name of the
   host string-view type of the first parameter and the names of its two
   members, from the binder.
1a. A mirror callback record that does not hold the view type and the
   two member names is rejected by the checker, with a provenance
   diagnostic that tells the user to generate the mirror again. The
   checker does not supply default member names. Each tier then gives
   the same acceptance.
2. For each callback typedef that the ship tier casts a trampoline to,
   the emitted C has a compile-time assertion that the host view and
   the runtime view have the same size, the same alignment, and the
   same offset and size of each member. The assertion uses `_Static_assert`,
   `sizeof`, `_Alignof`, and `offsetof`, which C11 compilers and `cl`
   accept.
3. The assertion is emitted once for each host view type in a C file.
4. The development tier does not change: the binder compares the view
   at bind time.
5. No program changes its output.

### 298.2 Acceptance

1. A ship-tier unit test emits the C of a program that binds a callback
   and finds the assertion of rule 2 for the host view.
2. A firing control: a test compiles the emitted C with a host view
   that has another member order, and the C compiler rejects it at the
   assertion. If the binder rejects that header before the C
   compiler, the test builds the HIR record directly and states it.
3. No corpus golden changes. The LIR text snapshot changes only in the
   callback provenance of a field (rule 1): 18 field lines. The full
   gate passes.

### 298.3 Phase Review

One fresh review of the implementation. No CRITICAL finding. The review
compared both generated mirrors with their regeneration tests, read the
`cl` flags (`/std:c11`) and the `stddef.h` include of the emitted C,
and ran the assertion through clang with `-std=c11 -pedantic`.

1. MAJOR, realistic, fixed: the parser accepted a callback record with
   no view type, and one with no member names (default `data` and
   `len`). A mirror from an older binder passed the checker and the
   development tier, and the ship build stopped with an internal
   error. Rule 1a now rejects the record in the checker: "callback
   provenance requires aggregate, data, and len; generate the mirror
   again". Each fixture moved to the full record.
2. MINOR, contrived, fixed: the assertion did not compare the size of
   each member. A host view with a `uint32_t` length kept the size of
   the struct on LP64. Rule 2 now compares each member size.
3. MINOR, contrived, fixed: the binder had an `expect` and an index in
   library code (core principle 5).
4. MINOR, realistic, fixed: the crate doc of the binder showed the old
   record.
5. MINOR, fixed: one spelled-out import, and a test directory that
   stayed after a failed assertion.

The first full gate failed in the LIR text snapshot test: the LIR text
prints the callback record, which now holds the view type and the
member names. The 36 changed lines are the 18 callback field lines.
The snapshot was captured again, and acceptance item 3 now states it.

### 298.4 Open

1. No `cl` run of the assertion on this host. The ship tier passes
   `/std:c11` to `cl`, which `_Static_assert` and `_Alignof` need in C
   mode *(docs)*.
