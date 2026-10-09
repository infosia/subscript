<!-- §187 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 187. A field with no read lowering is not read

*(Added 2026-10-10.)* Origin: a route measurement for the §142
amendment on 2026-10-09 (`specs/tracking/s187-descriptor-read.md`).
On 2026-10-10 the owner selected this section.

Problem: a struct field kind can have a script→C lowering and no
C→script lowering. The kinds are: a count-first pair (`size_t
itemsCount; const T* items;`, §30.2, §31.1), a string view reached
through a pointer member, and a callback field. §28 and §33.1 say that
a direction with no lowering fails loud. At `85c2fbcf` the binder and
the checker accept these fields in result positions. The run then reads
C bytes as script values:

| Form, at `85c2fbcf` | dev JIT | C AOT |
|---|---|---|
| by-value result of a struct with a pair (`Lay layGet(void)`) | internal lowering error | the process stops (SIGSEGV) |
| the same, pair nested by embedding or by a pointer member | the process stops | the process stops |
| pointer result `const Lay*` | the process stops | the process stops |
| string view one level down, through a returned pointer member | allocation of a wrong size fails | prints bytes from other memory |
| callback field in a by-value result (`SubCallbackInfo`) | internal lowering error | reads `userparam` as null where C wrote non-null |
| a struct with a callback field filled through an out-parameter | `true true` | `false false` |

The element type of a pair does not matter. The binder scan also misses
a by-value struct parameter as a root. `void fillLays(LayList list)`,
with a mutable pair of pair-holding elements, binds. The `const
LayList*` form is rejected.

### 187.1 Rules

1. **Read positions.** A read position is a place where C writes
   memory that the script reads after the call. The read positions
   are: a foreign-call result (by value or through a pointer), a
   completion result (§178 rule 6), a callback parameter, and a
   non-const pointer target that a parameter reaches. The parameter
   reaches it from a pointer root or a by-value root.
2. **No read lowering, no read.** A struct type in a read position that
   holds, at any depth (§32 embedding, §33 pointer members), a field
   with no C→script lowering is rejected. The diagnostic names the
   function, the position, and the innermost member, in the §32
   discipline. Under a fill that builds a scratch struct, a string
   view at any depth of by-value embedding has a read lowering. The
   copy-back materializes it (§30.1). Measured: the §179 boundary gate
   case `sweepNestedHalfWrite` writes `value->inner.label` and both
   tiers read it. C AOT reads a depth-2 view (`NBox`). A string view
   behind a pointer member has no read lowering at any root.
3. **One predicate, one total scan in the checker.** One predicate
   decides "this field has no C→script lowering". The predicate
   derives from the lowering record. The binder and the checker share
   it. The checker is the total scan. For every foreign function of
   every mirror, it walks each parameter and the result through the
   mirror types. Every struct that a position reaches is a class of
   some mirror, wherever its header defines it. The checker applies
   the predicate at each read position of rule 1. The mirror type
   does not show `const`. So every mirror records the `const` fact of
   each pointer parameter, each struct-pointer member, and each
   struct-element pair that it declares, in provenance comments. Every
   header emits these records, a declaration-only header included. A
   pointer parameter or member with no record (a hand-written mirror)
   counts as non-`const`. So the check fails closed. The read root of
   each position comes from the code-generation fact, not from a
   second derivation. One function decides, for each struct, whether a
   call copies its bytes or builds a scratch struct. For each pointer
   member, the same function decides whether the call passes the
   script memory or a scratch copy. The code generation of both tiers,
   the binder, and the checker call that function. A member that C can
   write and that the call does not write back has no read lowering.
   One walk visits every member that a root reaches (embedded structs,
   pair elements, pointer targets), whatever the route, and applies
   the predicate to each. The binder also rejects each position that
   it sees, as an early diagnostic with the same text. One scan over
   every declaration kind finds these positions. A new declaration
   kind does not compile until the scan classifies it. The mirror
   stays valid TypeScript that `tsc` accepts, with the same typing. A
   program never reaches code generation with such a read.
4. **The §30 out-parameter fill stays.** A direct count-first pair of a
   struct that a call fills through an out-parameter keeps the script
   array (`a100`). The copy-back does not read the pair, so the pair
   is not in a read position. A host that writes that pair changes
   nothing that the script reads. 187.3 item 1 records this.
5. **Write positions do not change.** A struct with these fields stays
   accepted as an input parameter (§28, §30.2, §31.1, §33.1).

6. **The tiers copy back the same members.** For each member kind
   under a fill, the dev JIT and C AOT write back the same members.
   Measured at `85c2fbcf`: C AOT writes back a `void *` userdata field
   under a scratch fill (`aa false`). The dev JIT skips it (from a read
   of `lower/func/boundary.rs`).

7. **Pointer targets are written back.** *(Owner decision,
   2026-10-10.)* When a call passes a pointer target as a scratch copy,
   both tiers write back the members of the target after the call.
   They do this at every depth, by the same copy-back as a fill (rule
   6). They keep the script link of each pointer member. Code at the
   call site writes back a target at a depth that the call knows at
   compile time. A pair whose elements need a write-back is rejected
   (rule 11), so no write-back has a count that only the run knows.
   The copy-back writes a member back only when C changed it. It
   compares each scratch member with the value that the call put
   there. So a script write that a callback makes during the call
   stays when C leaves the member unchanged. The tiers do not write
   back a target behind a `const` pointer. The `const` fact reaches
   code generation. A read-only host with a non-`const` pointer graph
   (`render(const Scene *scene)` with `Scene { Mesh *mesh; }`, `Mesh {
   Material *mat; }`) is accepted. A C write that replaces a pointer
   value itself is lost, as a write to a direct pair of a fill is
   (rule 4). 187.3 records it.
8. **Userdata and callback fields.** No registration exists to
   validate a C-written address. So a `void *` userdata slot or a
   callback field has no read lowering where the script reads C's
   bytes. The script reads C's bytes in a foreign result, and in a
   struct whose bytes the call passes as script memory. In a scratch
   struct the copy-back skips both fields, and the script keeps its
   own values. *(Owner decision, 2026-10-10.)* A scratch struct with
   these fields is accepted. A C write to them is lost (187.3 item 4).

9. **A struct cycle.** A struct that reaches itself through pointer
   members (`PNode { PNode *next; }`, or `A { B *b; }` with `B { A *a;
   }`) cannot pass as scratch copies. The pass decision gives a cycle
   outcome, and the binder and the checker reject each position that
   reaches it. Code generation never receives it. *(Owner decision,
   2026-10-10.)* A cycle whose structs all copy their bytes, through
   `const` pointers only, passes as script memory. The layouts are the
   same, and C cannot write it (`pSum(const PNode *)` with `PNode {
   int32_t v; const struct PNode *next; }`).
10. **A by-value struct whose bytes the call copies passes its bytes**,
   a fixed array of scalars included (`FT { float m[4]; int32_t k; }`).
11. **A pair at every reach.** A pair keeps the script array at every
   reach. C writes the elements in place, or into a scratch array that
   the call writes back (rule 7). A replacement of the pointer or the
   count is lost (187.3 item 4). So the predicate treats a pair the
   same at the fill root, at an embedded reach, and behind a pointer
   member. A pair of scalar elements is readable. A pair of struct
   elements is readable when its elements are readable by the one
   walk. A pair that C can write, whose elements the call builds as
   scratch copies, is rejected (core principle 16). The call copies
   each element in and back. The cost grows with the count, and the
   call site does not show it. The rule holds for a `const` pair too
   in one case. In that case, an element reaches, at any depth, a
   pointer member whose target C can write and that is a scratch copy
   (`renderScene(const Scene *s)` with `const Entity *entities`,
   `Entity { Mesh *mesh; }`, `Mesh { Material *material; }`). The
   diagnostic names that member and says that a `const` on it, or on
   a pointer between, makes the pair readable. A `const` pair whose
   elements reach no such member, and a pair of elements that copy
   their bytes, stay readable. A C-writable pair of elements that a
   read validates (a `CEnum` wire alias, §52) has no read lowering.
   The reason is that C writes the elements in place with no
   validation. A `const` pair of them stays readable.

12. **A result reads by its read fact.** A foreign result is a copy
   root. No call rebuilds it. Code generation of both tiers lowers a
   result by the read facts of its members, not by the
   write-direction pass of its struct. So a result struct with a
   fixed array and a pointer member (`FR { float m[4]; FP *p; }`)
   reads in both tiers, or the checker rejects it in both.

13. **A scalar fixed array is bytes.** A fixed array of scalars can be
   in a struct that the call builds as a scratch struct. Then both
   tiers copy the array as bytes, in the build and in the copy-back.
14. **An embedded-header link passes its declared type.** *(Owner
   decision, 2026-10-10, after a cost measurement.)* A link typed
   `H | null` passes as `H`, by the pass of `H`, whatever extension the
   box holds (§33 rule 9). The call reads no class id. A C cast to an
   extension that does not copy its bytes reads wrong values, or reads
   past a scratch copy of `H`. 187.3 records it. Measured before this
   decision: a class-id dispatch cost +9 ns per call for a boxed
   `Vec3` and +30 to 56 ns for `ent.pos` in C AOT. This cost applied
   to every math function whose header type some struct embeds first.
15. **One validation scope.** The binder's §28/§30 write-direction
   validation does not reject a member that the pass decision lowers.
   A scalar fixed array beside a pair or a string view
   (`PMesh { float bounds[4]; size_t idsCount; const uint32_t *ids; }`)
   binds, by rule 13.

### 187.2 Acceptance

1. Red first, at the contract pin: a reject entry for each result form
   of the problem table that the fixture can express. Each entry is
   accepted at the pin. `a100` and a control entry that passes the
   same struct as an input parameter stay accepted with their output.
2. Unit tests cover the predicate against each field kind at each
   depth. They cover the scan against each read position, including
   an out-parameter and a by-value root (core principle 9: the test
   builds the violating form). They cover the shared diagnostic text.
3. No accepted corpus entry changes its output.

### 187.3 Open

1. A host that writes the direct pair of a filled out-parameter loses
   that write without a diagnostic (rule 4). A read lowering for the
   pair (a copy into a new script array, as §28 rule 3 does for a
   string field) is a separate decision.
2. A by-value C array parameter (`void fillLays(Lay out[2])`) binds as
   `FixedArray<Lay, 2>`. C AOT fails to compile the call. The C
   compiler reports an incompatible parameter type. The same holds for
   a handle element. This is a binder by-value/pointer mismatch outside
   this section. In C the array parameter is a non-`const` pointer that
   the call fills. When the mismatch is fixed, the scan of rule 3 must
   classify this position.
3. A mirror that keeps `const=true` after its header changes the
   member or parameter to non-`const` is accepted. Nothing compares a
   mirror with its header at check time.

4. A C write that replaces the value of a pointer member is lost (rule
   7). A C write that replaces a direct pair of a fill is lost (rule
   4). A C write to a userdata slot or a callback field of a scratch
   struct is lost (rule 8). No diagnostic reports them. A C pointer
   cannot become a script object.

5. Rule 7 writes a pointer target and a pair element back by the fill
   copy-back. That copy-back can materialize a string view. But rule 2
   still gives no read lowering to a string view behind a pointer
   member or in a mutable pair element. So such a target is rejected.
   A measured relaxation is a separate decision.
6. A struct with a `FixedArray` of structs that a call rebuilds is
   rejected at code generation in both tiers with one text. The binder
   and the checker do not reject it first (the write-direction class of
   item 2).

7. If one call passes one script object to two fill parameters
   (`qTwoBump(t, t)`), the object gets the write-back of the last
   site. It does not get the C result of both writes (`11`, C gives
   `12`).

8. A struct whose bytes the call copies passes script memory. So a C
   write of a pointer member in it becomes a script reference to C
   memory (`Base { const struct Base *next; }` as an embedded header,
   measured `3 7`). This contradicts item 4.
9. Rule 7 cannot order a C write against a callback write to the same
   member. When both write, C's value stays. During a callback the
   script reads the values from before the call.
10. The rule 7 compare covers padding bytes. A C struct assignment
   with other padding counts as a change.
11. A nested written-back target allocates a scratch copy and opens a
   scratch scope in both tiers. A call-local copy is not measured.
   Code generation recomputes the pass decision on each query (400
   `createPipeline` call sites emit in 0.16 s, debug).

12. A hand-written mirror that declares a by-value descriptor of
   C-writable `CEnum` elements passes the checker. The descriptor
   record names the aggregate and the element type, not the pointer
   member. So the checker cannot give the binder's text. A generated
   mirror never reaches this. The binder rejects the header.

13. A C cast can go from an embedded-header link to an extension that
   does not copy its bytes. That cast reads the script layout of the
   extension, or reads past the scratch copy of the header (rule 14).
   Measured: `PExt { PBase base; PView name; }` gives `10000` (C
   expects `10003`). `PBase { int32_t kind; PView name; }` with `PExt {
   PBase base; int32_t extra; }` reads `extra` past the allocation.

### 187.4 Sections this one amends

- §33.1: "the read direction stays fail-loud" now names the positions
  of rule 1 and the predicate of rule 3.
