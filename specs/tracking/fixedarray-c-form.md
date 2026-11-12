# Each `FixedArray` type has a C form — `compiler.md` §254

Status: IN PROGRESS.

## Red at the pin

I built the debug CLI from `git archive 327ac0c4` in `$TMPDIR`, and ran
it on `$TMPDIR` copies:

- `a438-each-fixedarray-type-has-a-c-form.ts`: `subscript run` printed
  `sizes 0 0`. `subscript build --source … --run` failed the C compile
  with 2 errors: `program.c:111:61: error: use of undeclared identifier
  'SubFA_i32_2'` and `program.c:113:84: error: use of undeclared
  identifier 'SubFA_i32_3'`. Each one is a `sizeof` in an allocation.
- `r500-a-fixedarray-length-is-at-least-one.ts`: `subscript check` gave
  "no errors", and `subscript run` printed `a 0`. `subscript build
  --run` failed the C compile with 2 errors: "initializer for aggregate
  with no elements requires explicit braces", at `program.c:109:26` and
  `program.c:124:36`.

`tsc` 5.9.2 accepts both entries.

## After the change

- `a438` checks with no errors. The development tier, the ship tier,
  the interpreter, and `node` give `sizes 0 0`. `main` is not async, so
  the LIR text snapshot does not change. The C program has
  `typedef struct { int32_t a[2]; } SubFA_i32_2;` and
  `typedef struct { int32_t a[3]; } SubFA_i32_3;` before their use.
- `r500` gives one S100 "a `FixedArray` length is at least 1; C has no
  array with the length zero" at 8:32, the length of the result type,
  with the divergence block `FixedArrayLengthZero`.

## Form

- Rule 1: `compiler/src/check/tyres.rs` `resolve_type_ref` rejects the
  length `0` at the type, so each place of the type (an annotation, a
  type argument, a result type) gets the rule. The new site
  `FixedArrayLengthZero` (S100) has the divergence
  `FixedArrayLengthZero` (collision Q3) and the witnesses
  `s254-annotation` and `s254-result`. `collisions.md` Q3 states that
  the length is a positive integer literal, cites `r500`, and states that
  `tsc` accepts the length 0. `generic_tsc_matrix` reads the phrase
  "positive integer literal" in Q3.
- Rule 2: the emitter records each aggregate type whose C name it
  writes. `fixed_array_name` and `iter_result_name` are the only
  producers of the names `SubFA_…` and `SubIR_…`, and each one records
  its type in `named_aggregates`.
- `emit` writes the type text last, after the globals, the prototypes,
  the bodies, the helpers, and the worker adapters, and places it before
  them in `program.c`. In `emit_types`, the closure environment types
  and the frame types are written before the set is read, and placed
  after the aggregate types.
- The roots of the `typedef` set are the recorded types and each class.
  The order reads each element of a `FixedArray`, the value of an
  iterator result, and each field of a class. The `typedef` text of the
  set names only these parts, so it names no type outside the set.
- `emit_types` then compares the record with the set, and gives an
  internal error for a recorded type outside the set. So a name that
  the emitter writes in `program.c` before the set is read is in the
  set, and a later name that is outside the set stops the build.
- The other shape was to collect each type that the program holds,
  also the element types that the runtime holds. That shape lists the
  places of a type, so a new place can name a type outside the set.
  The record has no such list. I removed the former collection from the
  class fields, the globals, the function results, and the values,
  because the record covers each of these names.
- The `typedef` order follows the order in which the emitter names the
  types, and an aggregate type that no C text names gets no `typedef`.
  The bytes of each output do not change, and no golden holds the
  order of the C text.

## Files

- `compiler/src/check/tyres.rs`, `compiler/src/check/rejection_sites.rs`,
  `compiler/src/check/rejection.rs`,
  `compiler/src/check/rejection_witness_index.rs`,
  `compiler/src/check/rejection_witnesses.txt`,
  `compiler/src/divergence.rs`, `compiler/src/divergence/entries.rs`,
  `compiler/src/divergence/established.rs`
- `codegen/src/cemit.rs`, `codegen/src/cemit/emitter.rs`
- `compiler/tests/fixedarray_length.rs` (new),
  `codegen/tests/fixedarray_c_form.rs` (new, the nested form only; `a438`
  holds the forms of an empty array and a `Map`),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/generic_tsc_matrix.rs`,
  `compiler/tests/generic_tsc_matrix/api.rs`
- `corpus/accept/a438-each-fixedarray-type-has-a-c-form.ts` and
  `.expected`, `corpus/reject/r500-a-fixedarray-length-is-at-least-one.ts`
- `generated-docs/corpus-index.md`, `specs/blocks/collisions.md` (Q3)
- `specs/blocks/compiler/s218-a-nullable-string.md` (§218.4 item 2
  closed), `specs/blocks/compiler/s220-a-fixed-array-traversal-reads-its-place.md`
  (§220.4 item 2 closed)
