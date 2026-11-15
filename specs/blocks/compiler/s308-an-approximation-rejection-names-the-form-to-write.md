<!-- §308 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 308. An approximation rejection names the form to write

*(Added 2026-11-15.)* Origin: work unit 3 of the owner request of
2026-11-15 (`specs/tracking/divergence-alternatives.md`). The owner set
the order of the units on 2026-11-15.

Problem, measured at `23e6f607` by the survey. A `TscRejects` site
renders no `= subscript:` block (§154). Some of these sites approximate
`tsc`: `tsc` accepts some programs that reach them. For those programs
the output names no form that this checker accepts, or names a step
that the program already has.

| Site | Program that `tsc` accepts | Output | Form that the checker accepts |
|---|---|---|---|
| `NullableMember` (S011) | `if (h.f !== null) { while (true) { h.f.x; h.f = null; break; } }` | "narrow with a null check first"; the program has the check | `const f = h.f;` before the loop |
| `NullableCall` | `if (cb !== null) { while (true) { cb(1); cb = null; break; } }` | "is not callable" | `const c = cb;` in the check |
| `NullableNominalAssignment` (S005) | `if (f !== null) { while (true) { use(f); f = null; break; } }` | no step | a `const` copy |
| `MethodValueTruthTest` | `if (c.ready) { c.ready(); }` (`tsc` gives TS2774 only when the body does not call the method) | no step | `c.ready();` |
| `IncompatibleNominalAssignment` (S005) | `const p: P<i32> = new Q<i32>(1)`, `P` and `Q` with the same structure | no step | `new P<i32>(1)` |
| `DisjointLiteralAliasAssignment` | `if (a === "a") { const b: B = a; }` with `A = "a" \| "b"`, `B = "a" \| "c"` | no step | `const b: B = "a";` |
| `ClassReadBeforeDeclaration` and the three `Immediate…` sites of rule 5 | a closure that reads a later `const` | no step | the declaration before the use |

### 308.1 Rules

1. A narrowing fact that held at the entry of a loop, and that §162
   rule 3 or §304 ends at the loop head, gives its diagnostic at a site
   of its own for each of `NullableMember`, `NullableCall`, and
   `NullableNominalAssignment`. This applies only to a read that sees
   the end at the loop head: a store to the path, or a test of the
   path, earlier in the body flow before the read gives the site of the
   pin. The class stays `TscRejects`: `tsc` rejects the program when
   the store comes before the read. The note is: "The narrowing before
   the loop does not hold in the loop, because the loop stores this
   location. Copy the value into a `const` local before the loop. Or
   test the value for null again before this use." *(Amended 2026-11-15
   after the Phase Review, findings M1, m1, and m4.)*
2. `MethodValueTruthTest` gets the note "A method is never null. Call
   it without the test."
3. `IncompatibleNominalAssignment` gets the note "Classes are nominal.
   Make an object of the declared class." if the two classes differ, and
   "The type arguments differ. Use the type arguments of the declared
   type." if the class is the same. *(Amended 2026-11-15 after the
   Phase Review, finding m2.)*
4. `DisjointLiteralAliasAssignment` gets the note "Assign a literal
   member of the target alias." A test of the value does not change its
   alias, so the note does not name one. *(Amended 2026-11-15 after the
   Phase Review, finding M2.)*
5. `ClassReadBeforeDeclaration`, `ImmediatePendingLocalRead`, and
   `ImmediateNameWriteBeforeDeclaration` get the note "Move the
   declaration before the code that uses it." only when the declaration
   comes after the use in the source. `ImmediateShadowedNameRead` gets
   the note "A later local declaration hides the outer name. Give the
   local another name." *(Amended 2026-11-15 after the Phase Review,
   finding M3.)*
6. Each note is a fact and one instruction, in the style of the
   existing notes (§303 rule 1). No message line and no position moves.
7. A test checks each note: for each site of rules 1 to 5, a program
   that reaches the site, then each form that the note names, which the
   checker accepts.

### 308.2 Acceptance

1. Red first: the unit tests of rule 7 fail at the pin (no note, or no
   site of rule 1).
2. The §154 total test passes with the new sites of rule 1, each with
   a witness that `tsc` rejects (the store before the read).
3. No `.expected` golden moves. No program changes from accepted to
   rejected or back.

### 308.3 Phase Review

### 308.4 Open
