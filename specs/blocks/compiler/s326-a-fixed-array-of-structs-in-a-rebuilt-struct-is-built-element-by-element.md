<!-- §326 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 326. A fixed array of structs in a rebuilt struct is built element by element

*(Added 2026-11-18.)* Origin: §187.3 item 6, rank 5 of
`specs/tracking/backlog.md`.

Problem, measured at `4b136a0d`. A C struct that the call rebuilds as a
scratch struct (§187 rule 3) can hold a fixed array of structs. C
headers write this form for a small set of records
(`B { int32_t k; Q qs[2]; }`). No form of it runs:

| C declaration | `bind` | `check` | Code generation |
|---|---|---|---|
| `Q { int32_t x; size_t itemsCount; const int32_t *items; }`, `B { int32_t k; Q qs[2]; }`, `void fillB(B *b)` | binds `qs: FixedArray<Q, 2>` | no errors | internal lowering error: "boundary field `B.qs` is a fixed array of structs in a struct that the call rebuilds; no call lowering builds it" |
| the same, `int32_t readB(const B *b)` | binds | no errors | `boundary` prints "not lowered" for `b.qs`; the same internal error |
| `S { int32_t x; View name; }` (a string view), `B { int32_t k; S ss[2]; }`, `void fillB(B *b)` | rejected: "`S` field `name` is mirror-visible but the struct is not passed through any foreign pointer parameter" (false: `B` passes it) | — | — |
| `A { int32_t k; P ps[2]; View name; }`, `P` a struct that copies its bytes | rejected: "field `ps` is a fixed array of aggregates" | — | — |

§324 rule 2 already builds `N` scratch elements for a C array
parameter `T p[N]` and writes each one back. A fixed array field has
the same layout question, one level down.

### 326.1 Rules

1. **Each element is built.** In a struct that the call builds as a
   scratch struct, a field `FixedArray<T, N>` gets `N` elements in the
   scratch struct, at the C offsets of the C array. Each element is
   built by the pass of `T`: bytes for a `T` that copies its bytes, a
   scratch struct for a `T` that the call rebuilds (§187 rule 3), at any
   depth.
2. **Each element is written back.** If C can write the field (§187
   rule 7), the copy-back writes each element back by the same rules as
   a single struct field of type `T`, including the read lowering of a
   string view (§187 rule 2) and the pair read-back and compares of
   §325. A member path names the element index (`B.qs[1].items`).
3. **`const` is read only.** Behind a `const` pointer, the call builds
   the elements and writes nothing back.
4. **`bind` accepts the form.** `bind` stops rejecting a fixed array of
   structs in a struct that the call rebuilds, and it stops the false
   "not passed through any foreign pointer parameter" text for a struct
   that a fixed array field passes. An element type that has no pass
   (for example an array of pointers, §324) keeps a rejection that
   names the field.
5. **One plan.** The crossing plan of §187 rule 3 names the element
   pass of each such field. The binder, the checker, `boundary`, and
   both tiers read it. No form that `check` accepts reaches an internal
   lowering error.

### 326.2 Acceptance

1. Red first: an interop accept entry, with the fixture functions in
   `corpus/interop` and the mirror from `subscript bind`. It covers the
   four rows of the problem table: a pair element that C fills (each
   element's count and values), the same through a `const` pointer (C
   returns a sum), a string-view element that C replaces, and a
   byte-copied element beside a string view. At the pin, `bind`
   rejects or code generation stops.
2. A `boundary` text test shows the element pass of each field.
3. A bindgen test keeps the rejection of an element type with no pass,
   and the text names the field.
4. The CLI `check` output of each corpus entry is the same at the pin
   and after the change. LIR text goldens move only for entries that
   use the interop mirror.
5. The development tier and the ship tier print the same output.

### 326.3 Sections this one amends

- §187 rule 3 and rule 13: a fixed array of structs in a rebuilt struct
  is built element by element. §187.3 item 6 closes.

### 326.4 Phase Review

Form. The crossing plan gives a fixed array field one element plan per
element: `MemberCrossing::FixedArray` for the build,
`WriteBack::FixedArray` for scratch elements, and
`WriteBack::FixedArrayBytes` for elements that copy their bytes. The
binder, the checker, `boundary`, and both tiers read it. Each element
keeps the §325 pair read-back and compares, with the index in the path.

Measured with a476, development tier and ship tier the same:

| Form | Pin | After |
|---|---|---|
| Pair elements that C replaces | internal lowering error | `31 3 11 13`, `42 2 21 22` |
| The same through a `const` pointer | internal lowering error | `21 1 2` |
| String-view elements that C replaces | rejected by `bind` | `first second 51 62` |
| Byte-copied elements beside a string view | rejected by `bind` | `11 22 done` |

The CLI `check` output of 1,076 corpus entries is the same at the pin
and after. The LIR golden moved only for entries that use the interop
mirror.

One fresh review found no CRITICAL or MAJOR finding. Two MINOR findings
are fixed: `bind` rejected a whole header for a struct that no function
reaches (it bound at the pin), and a match arm could not run.

### 326.5 Open

1. The plan holds one element plan per element, and both tiers emit
   straight-line code for each element. `Vertex v[256]` gives 256
   copies and 256 compare blocks. The cost follows the declared data,
   but code size and compile time grow with `N`. A loop over one
   element plan is the cheaper form.
2. A hand-written mirror with `FixedArray<string, 2>` in a class that a
   call passes gets the §187 struct-result text, not a §326 text.
3. Contrived: the element offset arithmetic in
   `codegen/src/cemit/marshal.rs` and `codegen/src/lower/` does not use
   the checked layout helpers; only a very large array overflows it.
4. No `boundary` golden prints a "not lowered:" line for the reasons
   that remain (a callback with no userdata, a cycle, written-back
   elements).
