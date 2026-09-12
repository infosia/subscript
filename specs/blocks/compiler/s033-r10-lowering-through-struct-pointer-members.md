<!-- §33 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 33. R10 — lowering through struct-pointer members

Owner decision 2026-08-01 (downstream request R10, the last §32
recursion axis; blocking its render pipelines). §32's reachable set
covers by-value embedded aggregates and pair elements; a lowered
struct behind a **struct-pointer member** was not reachable.

### 33.1 The rule

Reachability extends through struct-pointer fields, recursively,
starting from a direct foreign descriptor-pointer parameter: a
boundary struct reachable only through `[nullable]` struct-pointer
members gets the same recursive script→C lowering as §32. At the
scratch construction, a non-null struct-pointer field lowers to a
pointer to a recursively-built scratch struct (call-duration valid,
like every §28 view and §32 scratch array); `null` lowers to
`NULL`. The read direction stays fail-loud, diagnostics naming the
innermost member (§32 discipline). The §30/§32 audits extend over
the pointer-reachable set: lowered-or-loud at any depth through any
mix of embedding, pair elements, and pointer members.

### 33.2 Corpus

`a106` (accept): the full render-pipeline composition at the
downstream depth — descriptor → nullable `fragment` pointer →
fragment state (string view + constants pair + `targets` pair whose
elements carry a nullable `blend` pointer to a plain struct) — with
both the null and non-null spellings exercised and a checker
returning evidence from every level including behind the pointers.
Goldens by the standard capture path.

### 33.3 Exit criteria (pre-registered)

1. `a106` byte-identical under both tiers, covering null and
   non-null pointer fields and the values behind them.
2. The verbatim evidence rejection at pin `aeaffcf` now mirrors and
   lowers (bindgen-unit-tested on that shape); pointer-reachable
   structs whose members still lack a lowering fail loud naming the
   innermost member.
3. The audits hold over the pointer-reachable set (extended
   `..._at_any_depth` coverage).
4. No existing golden moves; full gate, `tsc` gate, zero-warning
   sweep, and the generated-docs gates green.

### 33.4 The escape rule was written and withdrawn

*(Owner, 2026-08-28: written as a rule, then withdrawn the same day on
measurement.)*

**What the rule said.** A value whose type transitively holds a
nullable boundary-aggregate field may not escape the activation that
built it, rejected as S015. It was written after a round built two
storage classes for the defect below and a fresh review rejected each
one on an escaping shape.

**Why it is withdrawn.** Escape is not the discriminator. Measured at
`8b23e3c`, on the shape the consumer reports — an inner boundary
aggregate holding a string and two arrays, built in a conditional
expression, stored in the outer value's nullable field:

    returned from the function     selectors 3, 4, 5 read 0
    built and used in one function selectors 3, 4, 5 read 0

The two are identical. The rule forbids the escaping half of a defect
that fails the same way without escaping, so it fixes nothing.

**And it rejects a program that works.** `corpus/accept/a125` returns
a value with a nullable boundary-aggregate field from three functions.
Measured at the same pin, its value survives a 200-frame recursive
call after the return:

    no-clobber=1:12:34
    clobber=407628
    after-clobber=1:12:34

S015 rejected `a125` at four sites. The code is removed from the table
and no diagnostic ships.

**Where the escape framing came from.** The round that raised it wrote
its corpus entry with a direct return and an array return. Both
candidate storage classes then failed on those forms, and the round
reported that the language needed a storage-ownership rule. The
consumer never escapes: it builds a descriptor and passes it to a
foreign call.

**What is decided.** Nothing new. §33.1's call-duration scratch stands.

**Escape was measured wrongly, and the rule is reinstated.**
*(2026-08-28, after the Fable phase review of the post-§70 arc,
findings C1 and C2. This replaces a paragraph that read "Escape is
measured, and it works".)*

The withdrawal above rested on two measurements: `a125` survives a
200-frame recursive call after returning such a value, and a probe
that returned `a159`'s outer value printed the right 23 lines. **Both
are reads of a dead frame that happened to return the expected
bytes.** The storage behind a value-class-to-nullable address is a C
automatic in the emitting function's frame (`cemit.rs`, `(void*)&(operand)`;
`lower/func.rs`, a stack slot). Nothing in the contract gives it a
lifetime past that activation, and a measurement that reads a dead
frame does not discriminate.

The measurement that does discriminate, at `2a65724`: `setup()`
stores a holder whose descriptor carries a conditional fragment
temporary into a module global and returns; `main` reads it.

    dev tier    program terminated abnormally (signal 11)
    ship tier   global-before-clobber=0:0:0:0:8015:8016
                expected <sum>:12:1:1:31:47

The tiers disagree and neither is the program's meaning.

**Rule, reinstated as S015.** A value whose type transitively holds a
nullable boundary-aggregate field may not escape the activation that
built it. The escape sites are S009's: a `return`, an assignment to a
module-level binding, a store into an array element, a store into a
reference-class field, a capture by a lambda. The check is local to
one function; a value passed down is a copy, and the callee's own
check covers what the callee does with it. The traversal is §33's
reachable set; do not write a second one.

**`a125` is in this class and passes by luck.** Its three
`boundaryVia*` functions return a target whose `blend` names a
temporary in the callee's frame. The entry's purpose is conditional-arm
narrowing, which does not need the return: each function consumes the
target where it builds it. The rewrite keeps every printed line, so
`a125.expected` does not move. If a line must move, the round stops
and reports it.

The record of this section, in order: a rule written from an
unverified diagnosis; withdrawn on a measurement that did not
discriminate; reinstated on one that does. The middle step is the
defect, and it is CLAUDE.md's rule that a claim about behaviour
requires running the system in the shape that can fail.

Corpus: `r160` (the `setup()`/`main` shape above; `tsc` accepts).
`a125` rewritten. `a159` unchanged: it does not escape.

#### The defect itself

A value-class-to-nullable conversion takes the address of the source
aggregate's storage. `codegen/src/root_storage.rs` typed an
`l::ValueType::Address(_)` as zero root slots, so no address kept its
base alive. Before `74a091c` every managed value stayed a root for the
whole activation and the address survived by accident. `74a091c` made
the storage scope the live range (§68.2 rule 8), and the base then died
while an address into it was live.

**This is a defect inside §68's form.** LIR carries address
provenance, and root storage must read it: a base stays rooted while an
address derived from it is live. One shared plan serves both tiers, so
the fix has no per-transcriber site.

### 33.5 The script-side representation is a managed box

*(Owner, 2026-08-29; the box design was confirmed after the
recursion measurement.)*

§33.1 gives a non-null struct-pointer member a pointer to a scratch
struct at the call. It said nothing about how the script holds the
member between construction and the call, and the tree held it as an
**address into activation storage**. That representation is the root
of §33.4's whole record, of §68.2 rule 8b, and of the S015 escape rule:
each exists to keep an address alive, or to forbid the program shapes
where it cannot be kept alive.

**Measured at `857757a`:** the mirror declares a recursive member,
`SubChainHeader { next: SubChainHeader | null }`, the intrusive
extension chain of §12.3. An inline representation (payload plus a
presence flag) cannot hold a type that contains itself, so the box is
the one representation, not one of two.

**Rule.** A member, element, or local of type `T | null`, where `T` is
a boundary value class, holds either `null` or a **managed reference to
a heap copy of `T`** — a box. The box is a Context allocation with the
allocation header every managed object carries; it is freed by
`Context.collect()` or with the Context, as an array or a string is
(invariant 2: a program that never collects is correct, merely
larger).

1. **A store copies.** Storing a `T` value into such a place
   allocates a fresh box and copies `T` into it (C12 value semantics;
   no two places alias one box through a value store). Storing `null`
   stores `null`. Storing a `T | null` value copies the reference,
   as a reference-class handle copies.
2. **A read through the narrowed member is a place in the box.**
   After `x.f !== null`, `x.f.a` reads and `x.f.a = v` writes the
   box's storage, as a field of a reference class does. `const c: T =
   x.f` copies out.
3. **The foreign call reads the box.** §33.1's scratch construction
   takes the box's contents for a non-null member and `NULL` for
   `null`; nothing else changes at the call.
4. **No address of activation storage is taken.** The
   value-class-to-nullable conversion is an allocation and a copy,
   not an address. §68.7.2's boundary-address `Coerce` row goes; the
   form gains one instruction that takes a value-class datum and
   produces the box's handle (the round names it inside §68's rules,
   the verifier checks it, the interpreter implements it from the
   row).
5. **Recursion is representable.** A box holds a `T` that holds a
   box. The read direction stays fail-loud per §33.1.
6. **S015 is deleted**, and its code-table row with it. The escape it
   forbade is a copy of a reference, and it is sound. `r160` is
   deleted and its program becomes accept entry `a169`, printing every
   selector through the foreign checker after the escape.
7. **Rule 8b stays** for `AddressOfValue`, its remaining client (a
   by-value receiver). Its `Coerce` client is gone.
8. **What does not move.** `a106`, `a125` (as restructured), `a159`,
   `a163`, and the two fixtures restructured under S015 keep their
   goldens: an in-activation shape is unchanged by where the payload
   lives. `a125`'s original returning form is legal again, and a
   later round can restore it if the owner wants the entry in its
   first shape.

**Rules 9 and 10, the intrusive header.** *(Owner, 2026-08-29, after
round 8 measured the contradiction.)* `examples/e09`
and the two-header binding gate store `limit.engineHeader` — the first
field of an extension value — into `engineNext: EngineHeader | null`,
and the host casts the link back to `EngineEntityLimitOption*` and
reads the payload after the header. That is the C idiom the §12.3
chain pattern names: a pointer to the first member is a pointer to the
enclosing struct. Rule 1 as written boxes `T` alone and loses the
extension: measured, the ship tier read `0` where the golden holds
`2`, and the dev tier read `2` from memory past the box.

9. **An embedded header boxes its extension.** A boundary value class
   `H` is an *embedded header* when it is the first field of another
   boundary value class `E` (the *extension*) and some field or
   parameter is typed `H | null`. The checker derives this from the
   mirror; no annotation exists. Storing the place `e.h` — `e` a value
   of `E`, `h` its first field of type `H` — into an `H | null`
   location allocates the box with **`E`'s layout and class id** and
   copies all of `e`. The link's static type stays `H | null`, so the
   spelling is `tsc`-clean; a narrowed read through the link reads the
   `H` at the payload's start; the scratch construction hands the C
   side the payload pointer, which is the extension. Storing a value
   whose static type is `H` itself (a base header, constructed
   directly) boxes `H`, as rule 1 says.
10. **A chain header is not copied out of its extension.** A read of
   `e.h` as a value — `const h: H = e.h`, an argument, a return, a
   field of a value class — where `H` is an embedded header and `e` an
   extension, fails with S100 at the read: the copy would carry `E`'s
   tag with no `E` behind it, and the host would read past it. The
   only uses of `e.h` are the store of rule 9 and a read of `e.h`'s own
   fields. `tsc` accepts the copy; the narrowing is recorded here.

Corpus: `a169` adds the intrusive shape — an extension stored through
its header into a link, read back through the C checker's payload
selector after the building function returns. `r169` pins rule 10.
`examples/e09` and the two-header gate keep their sources and their
bytes.

*(Correction to the round's acceptance: the reference interpreter has
no foreign fixture, so `a169` joins the interpreter's declared
exclusion list with that reason, as `a106` does.)*

**Cost.** One allocation per non-null store, where the address
representation had none. The count of such stores in a program is the
count of descriptors it builds, which is small and not in any hot
loop the corpus measures; `perf-gate`'s two workloads build none.
`a163` gains a `live_bytes` line so the boxes are counted.
