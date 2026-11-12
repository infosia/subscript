<!-- §261 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 261. The reload fingerprint holds each layout input

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R02.
Selected by the owner on 2026-11-13.

Problem, read at `21fa0930`. Reload compares a declaration hash of the
old and the new module (§8.2). The text of a class in that hash
(`codegen/src/reload.rs`) holds the file, the name, the value or
reference kind, and the name and type of each field.
`Layouts::build` (`codegen/src/layout.rs`) computes the layout of a
class from four inputs: the value or reference kind, the boundary kind,
the explicit alignment of `@ValueType({ align: N })`, and the type of
each field. The hash does not hold the boundary kind or the alignment.

If an edit changes `@ValueType({ align: 4 })` to
`@ValueType({ align: 16 })` and no field, the hash does not change, and
reload accepts the new code. A reference object that the old code
allocated keeps the old offsets, and the new code reads and writes it
with the new offsets and fixed-array strides. A global reference is one
pointer slot in the two versions, so the check of the global block does
not see the change.

### 261.1 Rules

1. The text of a class in the declaration hash holds each input that
   `Layouts::build` reads for that class. The two read one record of the
   class, so a new layout input changes the hash with no second edit.
2. A change of the explicit alignment, of the boundary kind, or of the
   value or reference kind changes the hash, and reload refuses the new
   code as for a new field.
3. An edit that changes no layout input keeps the hash: a method body,
   and a constructor body.

### 261.2 Acceptance

1. Red first, at the pin: a unit test in `codegen/src/reload.rs`, in the
   form of `declaration_hash_changes_for_a_new_field`, hashes a value
   class with `align: 4` and the same class with `align: 16`. The hashes
   are equal at the pin.
2. After the change, the hashes differ, and `first_difference` names the
   class. The same test covers the boundary kind if a source program can
   change it with no other layout input. If no program can, the test
   states why.
3. A control: `declaration_hash_ignores_bodies` stays green.
4. No corpus entry or golden changes.

### 261.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each layout input that is not per class:
a `FixedArray` stride and a `Map` or `Set` entry stride come from the
field type and the hash entry of the element class, an enum width from
its member values, and the global block from the names, types, and
order of the globals. Each is in the hash.

1. MINOR, realistic, recorded: §261.4 item 1.
2. MINOR, realistic, fixed: the doc comment of `Builder` was on the
   class input record.
3. MINOR, contrived, fixed: the boundary test did not state why no
   program changes only the boundary kind, and it held an assertion
   that cannot fail.
4. MINOR, realistic, recorded: §261.4 item 2.

The quick gate of the first round found a test of §260 that failed:
two fixtures that ran in parallel made the same temporary directory.
Each fixture now has a directory of its own.

### 261.4 Open

1. An alignment that equals the natural alignment of the class changes
   no layout. If an edit adds or removes it, the hash changes, and
   reload refuses the new code.
2. The hash does not hold whether a field can be absent or has a
   default (`is_absence_capable`, `is_defaulted`). An edit from
   `name?: A` to `name: A` in a descriptor keeps the hash. An object
   that the old code made can hold the absence value of §43, which the
   new code does not expect. This is not measured.
