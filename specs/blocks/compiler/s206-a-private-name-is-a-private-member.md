<!-- §206 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 206. A private name is a private member

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected it as the sixth section of the second group. The
measurement is in `specs/tracking/private-names.md`.

Problem, measured at the pin `0ac9c22c`: a `#name` member gives S100
"class member form outside the decided surface" at the declaration.
Each access gives S100 "private names are not in the decided surface".
`tsc` 5.9.2 accepts the members, and `node` runs them. `collisions.md`
C24 row 11 gives "No lowering is decided" as the reason. The
`s154-undecided-sites` note T10 states that a run-time brand is not
necessary, because classes are nominal (C1).

### 206.1 Rules

1. A class can declare a `#x` instance field, static field, instance
   method, static method, getter, and setter, with `readonly` on a
   field. Each is a member that `private` declares, with the name
   `#x`. The rules of a member with that kind apply: for example, a
   `#x` write accessor needs a `#x` read accessor (§65.1).
2. The class body can read and write `this.#x`, `o.#x` where `o` has
   the class type, and `C.#s`. Code outside the class body fails with
   the existing text "member `#x` is private in class `C`". `tsc`
   rejects it with TS18013.
3. `#x` and `x` are two members of one class. Their C names are
   different, and no source name can give the C name of `#x`.
4. A `#x` field is a field of the C layout, in declaration order, as
   a `private` field is.
5. `JSON.stringify` omits a `#x` field, as JavaScript does.
   `JSON.parse<C>` fails with S100 "`JSON.parse` cannot set the private
   name `#x` of `C`" if `C` has a `#x` field.
6. These stay rejected: `#x in o`, `o?.#x` (`tsc` TS18030), a `#x`
   member of a `@Descriptor` class, and a `#x` member of a mirror
   class.
7. The form has no run-time cost: no brand check runs.

### 206.2 Acceptance

1. Red first, at the pin `0ac9c22c`: the accept entry `a391-private-names`
   is rejected. The reject entry `r452-private-name-outside-its-class`
   fails its expected error at the pin. The tracking note records each
   result.
2. `a391` passes `tsc`, and its output under `node` equals its golden.
   It uses each member kind of rule 1, a foreign receiver of the class
   type, `#x` beside `x`, and `JSON.stringify` of a class with a `#x`
   field.
3. A unit test pins rule 3: a class with `#x` and `x` gives two fields
   with different C names. A unit test pins rule 5 for `JSON.parse<C>`.
4. The rejection sites and divergence entries that only these forms
   reached are removed. The rejection-site total check passes.
5. `collisions.md` C24 row 11 drops the `#name` member.
6. No golden moves.

### 206.3 Phase Review

*(Added 2026-10-28.)* A fresh review of `0ac9c22c..06e24f05` gave no
CRITICAL or MAJOR finding, two MINOR findings, and three notes.

1. MINOR, contrived, fixed: a static and an instance member with one
   private name were accepted. `tsc` gives TS2804.
2. MINOR, realistic, fixed: the `JSON.stringify` text did not name the
   `#x` field whose type blocks the serialization.
3. Note, fixed: the member read and the assignment kept an arm for a
   private name that the front-end pass removes. A miss of the pass
   now gives an internal error.
4. Note, open: `#x in o` gives the general operator text.
5. Note, fixed: two sentences of this section did not follow STE.

Open:


1. `JSON.stringify` omits a `#x` field, but the type of that field
   must still be a JSON type. `JSON.stringify` rejects a class with a
   `#m: Map<…>` field; `node` omits the field.
