<!-- §322 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 322. A static readonly field keeps its narrowing

*(Added 2026-11-17.)* Origin: §304.4 item 1. The owner told the work to
continue down the open-item triage on 2026-11-16.

Problem, measured at `b384b8c4`. With `class A { x: i32 = 1; }`,
`mk(): A | null`, `tick(): void`, and
`class K { static readonly R: A | null = mk(); }`:

```ts
if (K.R !== null) { tick(); print(`${K.R.x}`); }
```

gives S011 "`A | null` may be null here" at `K.R.x`. `tsc` accepts it:
it keeps the narrowing of a `readonly` property across a call. §304
rule 3 keeps the fact of a module `const` global across a call, and
keeps a static field shared (C17).

### 322.1 Rules

1. A `static readonly` field is not a shared location for C17. Only the
   class initializer stores it, so a call, an `await`, and a store to
   another location do not end its fact, as §304 rule 3 states for a
   module `const` global. A lambda body does not keep its fact, as in
   `tsc` (TS18047 for `const read = (): i32 => K.R.x;` after the check,
   measured); §162 rule 4 does not apply to it. One exception, as in
   `tsc`: a lambda that is not async and is the callee of a call
   (§241 rule 2) keeps the fact. An async lambda that is called at once
   does not (TS18047, measured). *(Amended 2026-11-17 after the Phase
   Review: the round kept the fact in an async lambda called at once.)* *(Amended 2026-11-17:
   round 3 stopped because the first text kept the fact in a lambda
   body, which `tsc` rejects.)*
2. Rule 1 needs no fact about when a read runs. The class initializer
   stores the field once, and before that store the field holds the
   zero value of its type: `null` for a nullable type. So a null check
   that sees a non-null value runs after the one store, and no later
   code can store the field again. The init-order check of §71 (C14)
   also rejects each source read that can run before the store
   (measured: "`K.R` is accessed before its declaration, through
   `mk`"), so no accepted program reaches that state. A test that
   builds such a read in the HIR checks that each tier gives `null`
   there. *(Amended
   2026-11-17: round 1 stopped because the checker has no fact that a
   read can run during the class initializer; the first text of this
   rule asked for one.)*
3. A `static` field that is not `readonly`, and an instance `readonly`
   field, stay shared.
4. No program that `tsc` rejects becomes accepted. The development
   tier, the ship tier, and the interpreter give the same output. The
   accept entry carries `js-comparable` and equals `node`.

### 322.2 Acceptance

1. Red first: an accept entry with the program above, a narrowing kept
   across an `await`, and a lambda body that does not keep it (a
   reject test), with
   output equal to `node`. It is rejected at the pin.
2. Tests: a `static` field with no `readonly`, and an instance
   `readonly` field, keep S011 after a call (controls).
3. The CLI `check` output of each corpus entry is the same at the pin
   and after the change, except the new entry.

### 322.3 Phase Review

Three rounds stopped on the contract before the implementation landed:
rule 2 asked for a fact about when a read runs, which the checker does
not have (rule 2 now rests on the one store, the `null` before it, and
the init-order check of §71); the init-order check rejects the source
form of the rule 2 test (the test builds the read in the HIR, and each
tier gives `null`); rule 1 kept the fact in a lambda body, which `tsc`
rejects.

One fresh review found one MAJOR finding, realistic: an async lambda
called at once kept the fact; `tsc` gives TS18047. Rule 1 is amended
(r539, r540). Two MINOR findings are fixed: the C17 text named each
static field as shared, and r538 cited C7. The review probed a call, a
loop, a generator `yield`, a getter, a static method, a sync lambda
called at once, and a hot reload that flips `readonly` (the reload hash
refuses it). Over 1,143 corpus files, only a472 differs from the pin.

### 322.4 Open

None.
