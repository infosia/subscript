<!-- §294 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 294. The reload fingerprint holds the absence of a field

*(Added 2026-11-15.)* Origin: §261.4 item 2. On 2026-11-15 the owner
told Claude to do each item of a list of open runtime defects, in an
order that Claude selects. This item is one of them.

Problem, read at `f8e6d150`. The text of a field in the declaration
hash (`codegen/src/reload.rs`) holds the index, the name, and the type
of the field. A descriptor field `name?: A`, where `A` is a Q32 alias,
is absence-capable (§43): if a literal omits it, the object holds the
reserved absent discriminant. A field `name?: A = "a"` and a field
`name!: A` are not absence-capable: each read gives a member of `A`.
The hash does not hold `is_absence_capable`.

If an edit changes `compare?: CompareFunction` to
`compare?: CompareFunction = "never"`, the hash does not change, and
reload accepts the new code. An object that the old code made can hold
the absent discriminant. The new code reads that field as a member of
the alias, with no test for `undefined`.

### 294.1 Rules

1. The text of a field in the declaration hash holds each fact of the
   field that limits the values that an object of the class can hold:
   the index, the name, the type, and `is_absence_capable`.
2. An edit that adds or removes absence on a field changes the hash,
   and reload refuses the new code, as for a new field.
3. `is_defaulted` does not enter the hash. A default runs only when
   code makes an object. An object holds a member value with a default
   and with no default. So an edit from `name?: A = "a"` to `name!: A`,
   or the reverse, keeps the hash, and reload accepts it.
4. A field fact that only the code that makes or checks an object reads
   does not enter the hash: the initializer, `written_non_null`, the
   foreign provenance, and the position. The implementation states this
   list beside rule 1, from the fields of `hir::Field`, so a new field
   of `hir::Field` needs a decision in the same edit.

### 294.2 Acceptance

1. Red first, at the contract commit: a unit test in
   `codegen/src/reload/tests.rs`, in the form of
   `declaration_hash_changes_for_a_new_field`, hashes a descriptor with
   `compare?: CompareFunction` and the same descriptor with
   `compare?: CompareFunction = "never"`. The hashes are equal at the
   contract commit. After the change, they differ, and
   `first_difference` names the class.
2. A control for rule 3 in the same file: `name?: A = "a"` and
   `name!: A` give equal hashes.
3. A reload test in `codegen/tests/reload.rs`: the old code stores a
   descriptor with an omitted absence-capable field in a module
   global, and the new code gives the field a default. Reload refuses
   the new code, and the old code keeps running. The tracking note
   records what the new code read at the contract commit.
4. No corpus entry or golden changes.
5. The full gate passes.

### 294.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each fact that limits a stored value:
the absent discriminant comes from the alias, whose members and wire
values are in the hash; a removal of `@Descriptor` changes
`is_absence_capable`; `is_defaulted` has no reader in the runtime.

1. MINOR, realistic, fixed: the control of rule 3 passed if its edit
   did not apply.
2. MINOR, fixed: a test comment stated the behaviour before the change.
3. MINOR, fixed: rule 4 had no check. A compiler unit test now
   destructures `hir::Field` with no `..`, so a new field fails the
   build.
4. MINOR, fixed: the doc comment of `declaration_hash` did not name
   the absence of a field.
5. MINOR, contrived, recorded: §294.4 item 1.

### 294.4 Open

1. The boundary mirror facts `ClassDef::callback_lifetime`,
   `boundary_header`, and `Field::foreign_provenance` are not in the
   hash. Each comes from a mirrored `.d.ts`, which a reload edit does
   not typically change.
