# §187 — a descriptor field in a read position

Contract: `specs/blocks/compiler/s187-a-field-with-no-read-lowering-is-not-read.md`.

## Measurement at `9963cfc3` (route table of the §142 amendment)

Header shapes bound with the project binder, then checked and built
(C AOT) at `9963cfc3`:

```c
typedef struct Lay { int32_t tag; size_t itemsCount; const Obj *items; } Lay;
Lay layGet(void);
void layFill(Lay *out);
typedef struct Arr { size_t itemsCount; const Obj *items; } Arr;
void arrFill(Arr *out);
```

| Form | Result |
|---|---|
| `const l = layGet(); print(`${l.items.length}`)` | bind and check accept; the C AOT process stops, exit 139. The emitted C copies the C struct bytes into the script struct (`memcpy`) and reads the count and pointer bytes as an array header. |
| the same with `const float *values` | the same result |
| `layFill(l)` after the host writes 2 elements | `l.items.length` reads 0; no diagnostic |
| `arrFill` | binds as `arrFill(out: Obj[] \| null)`; the checker rejects the mirror (S011, S100) |

The dev JIT was not measured for these forms.

Correction: the `layFill` row is not a defect. A fill keeps the script
array of a direct pair (§30, `a100` pins it), so `length` 0 is the
length of the script array before the call. The `arrFill` row is the
pointer-first descriptor; a count-first `Arr` binds as `Arr | null`.

## Round 1 measurement at `c888a576` (both tiers, native fixture)

| Form | dev JIT | C AOT |
|---|---|---|
| control: struct with a pair as a by-value input | `31` | `31` |
| by-value result | internal lowering error ("foreign aggregate return contains an absorbed field") | SIGSEGV |
| out-parameter fill | `5`, `0` | `5`, `0` |
| pair nested by embedding, by-value result | signal 11 | SIGSEGV |
| pair nested by a pointer member, by-value result | signal 11 | SIGSEGV |
| pointer result `const Lay*` | signal 11 | SIGSEGV |
| pair nested by embedding or a pointer member, fill | rejected by the binder | rejected by the binder |
| string view one level down, through a returned pointer member | allocation of 8391173023195161197 bytes fails | prints bytes from other memory |
| callback field in a by-value result | internal lowering error | `userparam` reads null where C wrote non-null |
| callback-field struct filled through an out-parameter | `true true` | `false false` |
| `void*` field in a result | both tiers agree | both tiers agree |
| plain struct-pointer member, plain pointer result | both tiers agree, also after `Context.collect()` | the same |
| `void fillLays(LayList list)`, mutable pair of pair-holding elements | binds | — |

A strict rule that rejects every out-parameter fill with a pair rejects
`interop.h` as a whole (`a100`, `subProbeTextureDescriptorFill`). The
contract keeps the §30 fill (rule 4).

## Implementation

### The predicate

`subscript_boundary::member_read` (`boundary/src/read.rs`) decides one
member from its lowering class, the read root, and the depth. The
binder (`bindgen/src/read_lowering.rs`, `member_kind`) derives the class
from the registry `Kind` and the pair recognizer `embedded_array_pairs`.
The checker (`compiler/src/check/foreign_read.rs`) derives it from the
mirror type. Both report with `no_read_lowering_message`.

| Member class | Copy root, any depth | Scratch root, depth 0 | Scratch root, depth 1 or more |
|---|---|---|---|
| bytes (scalar, enum, handle, userdata, alias, external, scalar fixed array) | readable | readable | readable |
| count-first pair, scalar elements | no read lowering | readable (rule 4) | no read lowering |
| count-first pair, struct elements | elements, then no read lowering | `const`: readable; mutable: elements | elements, then no read lowering |
| string view | no read lowering | readable (§28.2 copy-back) | no read lowering |
| callback | no read lowering | no read lowering | no read lowering |
| embedded descriptor aggregate | no read lowering | no read lowering | no read lowering |
| embedded struct, fixed array of structs | nested struct | nested struct | nested struct |
| struct-pointer member | nested struct | nested struct, then no read lowering | nested struct, then no read lowering |

"Then" means: if no nested member is unreadable, the member itself is.
The diagnostic names the innermost unreadable member. A root that is
itself a descriptor aggregate or a string view has no read lowering.
The copy root is a result, a completion result, a callback parameter,
or a fill of a struct that needs no scratch struct. The scratch root is
a fill of a struct that needs a scratch struct.

### The read positions

`read_lowering::validate` matches every `Decl` variant with no wildcard
arm:

| Position | Root | `ReadPosition` |
|---|---|---|
| `Decl::Func` result, by value or through a pointer | copy | `Result` |
| non-`const` struct-pointer parameter (a fill) | scratch if the struct needs one, else copy | `Parameter` |
| mutable pair elements, reached from a by-value or a pointer parameter, at any depth | scratch, depth 1 | `Reached` |
| non-`const` struct-pointer member target, reached from a by-value or a pointer parameter, at any depth | the parameter root's kind, depth 1 | `Reached` |
| `Decl::FnPtr` parameter of a reachable callback, except a by-value string view | copy | `CallbackParameter` |
| completion result (`completion::select`) | copy | `CompletionResult` |
| `Decl::Struct`, `Decl::Enum`, `Decl::Handle` | no position | — |

The callback shape rule admits only `(StringView, void *, void *)`, so no
header reaches the callback-parameter position today. The unit test
reads it on a parse that has not passed that rule.

The checker reads each foreign result (and each completion result)
after every mirror resolves its classes, so a class that a later mirror
declares is visible. Its site is `RejectionSite::ForeignResultRead`
(S100). Its class is `Diverges(Divergence::ForeignResultRead)`: `tsc`
accepts the form (witness `foreign-result-read`, measured by the §154
witness test). The divergence entry shows a result of a struct with
scalar fields as the accepted form. A completion result class admits only scalar,
enum, and nested struct fields (§178 rule 6), so the checker finds
nothing there today.

The direct string-field result check in `emit.rs`
(`returns string-field boundary struct`) runs before the scan and keeps
its text. The predicate also rejects that form.

### Corpus

The reject entries are `corpus-ambient` second mirrors. Each declares
a result of a struct that `interop.h` declares, marked
`@subscript-c-external` (§48). The interop fixture gains the structs
and two input functions (`subDescReadLayTotal`, `subDescReadStrTotal`).

| Entry | Form | Pin `7891a753`: check, dev JIT, C AOT, interpreter | Now |
|---|---|---|---|
| r405 | by-value result, direct pair | accepted; empty stdout on each tier | S100, `SubDescReadLay.items` |
| r406 | pair nested by embedding | the same | S100, `SubDescReadLay.items` |
| r407 | pair nested by a pointer member | the same | S100, `SubDescReadLay.items` |
| r408 | pointer result | the same | S100, `SubDescReadLay.items` |
| r409 | string view through a returned pointer member | the same | S100, `SubDescReadStr.label` |
| r410 | callback field in a by-value result | the same | S100, `SubCallbackInfo.callback` |
| a363 | control: the structs as input parameters | `31 5 16 -1` on dev JIT and C AOT | unchanged |
| a100 | §30 fill keeps the direct pair | golden output on dev JIT and C AOT | unchanged |

`tsc` 5.9.2 accepts each reject entry with the prelude and the interop
mirrors (exit 0). The pin runs used the new fixture files with the
`7891a753` source tree. The callback-field fill row is a binder unit
test: the checker cannot see the `const` of another mirror's pointer
parameter (187.3 item 2).

Four bindgen message tests in `bindgen/tests/provenance.rs` take the
shared text. Each names the same innermost member as at the pin.

### Tests and costs (debug build, arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `read::tests` (2) | under 0.01 s | each member class at each root and depth; the text of each position |
| `read_lowering::tests::predicate_finds_each_member_kind_at_each_depth` | 0.01 s, one libclang parse | each member kind at depth 0, 1 (embedded), 2 (pointer member), at both roots; mutable pair elements; absorbed roots; a pointer cycle |
| `read_lowering::tests::scan_rejects_each_read_position` | 0.04 s, two parses per case, 8 cases | result by value and by pointer; fill without and with scratch; rule 4; a by-value root and a pointer root; a non-`const` member; each against an accepted twin |
| `read_lowering::tests::scan_rejects_a_completion_result` | 0.01 s, two parses | completion result, against a scalar twin |
| `read_lowering::tests::scan_rejects_a_callback_parameter` | 0.01 s, two parses | callback parameter, against a plain twin |
| r405–r410 in `corpus_reject` | 18–24 ms each, one check | the checker position |
| a363 in `golden` | dev JIT 37–62 ms, C AOT 1.0–1.7 s | rule 5 on both tiers |

The LIR text golden (`codegen/tests/lir-goldens/corpus.txt`) moves:
the interop mirror declares five more classes and two more foreign
functions, so later class, field, and foreign ids shift. No other line
changes.

## Phase Review 1 (2026-10-10)

MAJOR: a §48 split header (`a.h` declares `Lay`, `Outer`, `Holder`;
`b.h` includes it, marks them `@subscript-external`, and declares
`void outerFill(Outer *out)` and `void holderTouch(Holder h)`) binds and
checks; C AOT prints `42 5 2` and `3 1` after C writes the nested pair
and the pointer-member target, with no diagnostic. A callback-field
fill (`void infoFill(Info *out)`) runs the script closure after C wrote
`out->callback`. The binder of `b.h` saw each struct as `External`, with
no fields. The same declarations in one header are rejected. Rule 3
now classifies an external struct by its definition in the binder's
own parse.

### Fixes after Phase Review 1

MAJOR (split header). The frontend keeps the `typedef` declarations of
included headers that are not system headers (`Parsed.included_decls`,
`included_aliases`, and `included_unmodeled` for a `typedef` it does
not model). The read scan builds a view of the parse: the included
`typedef`s join the main declarations, and the view declares no
external type. The registry of the view classifies an external struct
by its definition. An external with no visible definition stays
unregistered, so it is opaque. An external that reaches an unmodeled
included `typedef` is a bind error. Emission still sees the external
as `Kind::External`.

The split-header program: `a.h` declares `Lay`, `Outer`, `Holder`
(`Lay *lay`), and `Info` (a callback field). `b.h` includes `a.h`,
marks `Outer`, `Holder`, and `Info` external, and declares
`outerFill(Outer *out)`, `holderTouch(Holder h)`, and
`infoFill(Info *out)`. The C fills write `inner.itemsCount = 3`,
`lay->tag = 9`, `lay->itemsCount = 3`, and `userdata1`.

| Build | `b.h` bind | C AOT output |
|---|---|---|
| CLI at `7891a753` | binds | `42 5 1` (C wrote 3 items), `1 3 1` (C wrote tag 9 and 3 items), `false` |
| CLI now | rejected: `outerFill` parameter `out` reads `Lay.items` | — |

Each position alone is rejected now: `holderTouch` parameter `h`
through `Holder.lay` reads `Lay.items`; `infoFill` parameter `out`
reads `Info.callback`. `a.h` binds to the same mirror before and after.

minor (divergence block). The result-read rejection has its own site
and divergence entry (above). The entry no longer says to return data
through a mirrored boundary struct.

minor (wildcard). The mirror-type classification in
`compiler/src/check/foreign_read.rs` matches every `Type` variant.

minor (a363). Its `questions:` line names compiler.md §187.

| Test | Cost | What it proves |
|---|---|---|
| `bindgen/tests/read_split_header.rs` | 0.06 s, one temporary header, eight libclang parses | each split-header position against an accepted twin; an undefined external is opaque; an unmodeled included definition fails loud |


### A forward-declared external

`a.h` defines `Outer { int32_t kind; Lay inner; }`, with a pair in
`Lay`. `b.h` does not include `a.h`. The C fill writes `kind = 42`,
`inner.tag = 5`, and 3 items.

| `b.h` form | Round 3 tree (before) | Now |
|---|---|---|
| `typedef struct Outer Outer;` plus the external directive | bind error at `7891a753` and now: the external is also defined in this header | the same |
| external directive, no declaration: `void outerFill(Outer *out);` | binds; check accepts; C AOT prints `42 5 1` (C wrote 3 items) | bind error: parameter `out` points to external type `Outer`, which has no definition |
| the same with `Outer *outerGet(void);` | binds; the checker rejects the result (S100, `Lay.items`) | bind error: the result points to `Outer` |
| `struct Outer;` forward tag, `struct Outer *` positions | binds; the same as the row above | bind error |
| `void outerUse(const Outer *in);` | binds | binds |

The read view keeps `Kind::External` for an external with no
definition. A non-`const` pointer parameter, a pointer result, a
non-`const` pointer member reached from a parameter, and a pointer
member of a read root to such a type are bind errors. A `const`
pointer parameter and a `const` pointer member of an input struct stay
accepted. A by-value use stays accepted: the parse cannot tell an
opaque handle typedef (`SubDevice`) from a struct by value. No committed
header uses a non-`const` pointer or a pointer result of an external;
the bindgen regeneration tests for `external-device.h`,
`host-completion.h`, and `host-buffer-completion.h` pass.

| Test | Cost | What it proves |
|---|---|---|
| `read_split_header::a_pointer_to_an_undefined_external_fails_loud_where_c_writes` | under 0.04 s, seven in-memory parses | fill, pointer result, and non-`const` member, each against a `const` twin; a pointer member of a result struct |
| `boundary` `read::tests::no_definition_message_names_the_position_and_the_external` | under 0.01 s | the root diagnostic text |

## Phase Review 2 (2026-10-10)

MAJOR: an external with no definition in the binder's parse, embedded
by value in a fill root or passed by value, reaches code generation.
`b.h` marks `Info` external without the include (the
`external-device.h` pattern) and declares `typedef struct Box { int32_t
k; Info info; } Box; void boxFill(Box *out);`; C writes
`out->info.userdata1`. Bind and check accept; C AOT prints `4 false`
and stops (exit 139); `infoCall(b.info)` instead gives trap 21. A
by-value `Holder` with a pointer member prints `1 1` after C writes
through it (no copy-back), and a `Box { Outer outer; }` fill drops the
pair (`4 5 6 1`). The binder cannot tell a handle from a struct for an
undefined external; the checker can, but the mirror spells `T*` and
`const T*` alike. Rule 3 now has the binder record the read-root fact
of each such position in the mirror, and the checker applies the
predicate.

Round 4 measurement: binding `a.h` with `Holder { int32_t k; Lay *lay; }`
and with `const Lay *lay` gives two mirrors that differ only in the
header file name (`lay: Lay | null` in both). The checker cannot tell
e1 from its `const` twin, so rule 3 now has the defining mirror record
member mutability.

### Fixes after Phase Review 2

Correction: the "Now" column of "A forward-declared external" is
superseded. An undefined external is no longer a bind error at any
position. The binder records the position, and the checker decides it.

Records (provenance comments, so `tsc` reads no new type):

- `// @subscript-c-read function="f" parameter="p" root="fill"`, in the
  mirror whose header uses an external with no definition in its parse.
  One record for each result or parameter that reaches such an
  external, directly, embedded by value, through a pair, or behind a
  pointer member. `root` is `fill` (non-`const` pointer), `const`
  (`const` pointer), `value` (by value), or `result` with
  `parameter="return"` (a C keyword, so no parameter has that name).
- `// @subscript-c-member aggregate="A" member="m" const=true|false`, in
  the mirror that defines `A`: one record for each struct-pointer member
  and each pair of struct elements. The record carries both values, so a
  member with no record (a hand-written mirror) counts as mutable with no
  conflict.

The checker reads each recorded parameter with the mirror classes. A
fill reads the whole struct at the scratch root when the mirror types
need a scratch struct (a pair, a string view, or a nested struct that
needs one), else at the copy root. A `value` or `const` root reads the
targets of the mutable pointer members and pairs. A by-value descriptor
reads its elements when its descriptor record says `const=false`. Every
result is read, as before. Seven record sites are new, each with a
witness and a divergence entry: `ProvenanceEmptyMember`,
`ProvenanceDuplicateMember`, `MirrorMemberTargetMissing`,
`ProvenanceEmptyRead`, `ProvenanceDuplicateRead`,
`ProvenanceInvalidReadRoot`, `MirrorReadTargetMissing`. `tsc` accepts
each witness (the §154 witness test).

Regenerated mirrors whose output changes (records only, no declaration
line moves): `interop.generated.d.ts` (33 member records),
`external-device.generated.d.ts` (3 read records),
`host-completion.generated.d.ts` (5), `host-buffer-completion.generated.d.ts`
(3), `examples/engine/engine.generated.d.ts` (1 member record).
`boundary-values`, `wire-enum`, and `benchmarks/boundary-noop` do not
change. With the record lines removed, each mirror equals its committed
text, apart from the §187 fixture classes. `tsc -p tsconfig.json`
(all interop and example mirrors) exits 0.

| Probe | Before (Phase Review 2) | Now |
|---|---|---|
| e1 `holderTouch(Holder h)`, `Lay *lay` | C AOT `1 1`, the write is lost | rejected: parameter `h` through `Holder.lay` reads `Lay.items` |
| e1 `holderTouchC(const Holder *h)` | the same | rejected, the same text |
| e1 twin, `const Lay *lay` | — | accepted |
| e4 `boxFill(Box *out)`, `Box { Outer outer; }` | C AOT `4 5 6 1`, the pair is dropped | rejected: parameter `out` reads `Lay.items` |
| e4 twin, `boxUse(const Box *box)` | — | accepted |
| e5 `boxFill(Box *out)`, `Box { Info info; }` | C AOT `4 false`, exit 139 | rejected: parameter `out` reads `Info.callback` |
| e5 twin, `boxUse(const Box *box)` | — | accepted |
| `SubDevice` by value (`external-device.h`, a127) | accepted | accepted: a handle is no struct |

Corpus r411 (`value` root, mutable member), r412 (fill, embedded pair),
and r413 (fill, embedded callback) are corpus-ambient second mirrors
over `SubDescReadMutHolder`, `SubDescReadOuter`, and `SubCallbackInfo`.
`tsc` 5.9.2 accepts each (exit 0).

Pin evidence at `7891a753`: the pin copy takes this tree's `interop.h`
and `interop.c`, and the pin binder regenerates every interop mirror, so
the mirrors carry only what the pin emits (no `member` or `read`
record).

| Entry | Check, dev JIT, C AOT, interpreter at the pin |
|---|---|
| r405–r410 | accepted; empty stdout on each tier (Red) |
| r411–r413 | rejected at check, line 11: unknown record kind `read`; not Red |
| r411–r413 with line 11 removed | check exit 0 |

r411–r413 are not Red at the pin: the pin rejects the `read` record
that is part of their form. With the record removed, each is the mirror
that the pin binder emits for the split header, and the pin accepts it.

| Test | Cost | What it proves |
|---|---|---|
| `compiler/tests/foreign_read_records.rs` (3 tests) | 0.01 s, in-memory sources | e1 at a value and a `const` root, with no record and with a `const=true` twin; e4 and e5 fills against `const` twins; a handle by value, with a struct control |
| `read_split_header::an_undefined_external_position_records_its_read_root` | one in-memory parse | each root fact, an embedded external, a pointer member |
| `read_split_header::the_defining_mirror_records_member_const` | one in-memory parse | member records for pointer members and struct pairs only |
| r411–r413 in `corpus_reject` | one check each | the corpus form |

## Phase Review 3 (2026-10-10)

Three MAJOR findings on the record design of rule 3 (records only for
undefined-external positions):

- An external reached through an included header (`c.h` defines `Lay`;
  `a.h` marks it external and declares `Outer { struct Lay *lay; }`;
  `b.h` includes `a.h`) is classified as bytes: no record, check
  accepts; C AOT prints `42` / `1 3` after C writes the pointer and its
  target.
- A declaration-only defining header emits no member records, so a
  `const` member counts as mutable and valid programs are rejected.
- A hand-written mirror with no read record fails open: a nested-pair
  fill reaches code generation (`4 5 1`).

Rule 3 now makes the checker the total scan over every foreign
function of every mirror, with `const` records for every pointer
parameter and member, emitted by every header, failing closed when a
record is missing.

### Fixes after Phase Review 3

Correction: the `@subscript-c-read` records of "Fixes after Phase
Review 2" are removed. The checker is now the total scan, so a record of
which positions to read is not needed.

Records (provenance comments):

- `// @subscript-c-parameter function="f" parameter="p" const=true|false`
  for each struct-pointer parameter (`Boundary` or external pointee).
- `// @subscript-c-member aggregate="A" member="m" const=true|false` for
  each struct-pointer member and each pair of struct elements.

Every header emits them. A declaration-only header emits only its member
records, with no header record, so code generation sees no new header.
A pointer parameter or member with no record counts as non-`const`.

The checker reads, for every foreign function of every mirror: the
result (copy root); each `X | null` parameter, as a fill unless its
record says `const=true`; each `const` pointer and by-value struct, as an
input (the targets of mutable pointer members and pairs); each by-value
descriptor, whose elements are read unless the descriptor record says
`const=true`. It also reads the struct arguments of every callback field
of a boundary class. A function type has no parameter names, so that
position names the argument by index (`#0`). The binder's scan stays as
the early diagnostic.

Record sites: `ProvenanceEmptyPointerParameter` and
`ProvenanceDuplicatePointerParameter` replace the read-record sites.
`ProvenanceInvalidReadRoot` and `MirrorReadTargetMissing` are removed. A
pointer-parameter record that names no parameter takes the existing
`MirrorParameterTargetMissing`.

Regenerated mirrors whose output changes (record lines only):
`interop.generated.d.ts` (33 member, 31 parameter records),
`wire-enum.generated.d.ts` (4 parameter records),
`examples/engine/engine.generated.d.ts` (1 member, 1 parameter record).
`external-device`, `host-completion`, and `host-buffer-completion` are
again byte-identical to `HEAD`. With the record lines removed, each
mirror equals its committed text, apart from the §187 fixture classes.
`tsc -p tsconfig.json` exits 0. Two codegen fixtures with hand-written
mirrors take records that state the `const` of their C side:
`boundary_scratch_breadth.rs` (`scratchObserve*` reads its argument).

| Finding | At `7891a753` with pin-binder mirrors | Now |
|---|---|---|
| M1: `c.h` defines `Lay`; `a.h` marks it external, `Outer { struct Lay *lay; }`; `b.h` includes `a.h`: `outerFill(Outer *out)`, `outerTouch(Outer o)`, `outerUse(const Outer *o)` | check accepts all three (the reviewer's C AOT: `42` / `1 3`) | rejected: `outerFill` reads `Lay.items`; `outerTouch` and `outerUse` through `Outer.lay` read `Lay.items`; with `const struct Lay *lay`, only the fill is rejected |
| M2: declaration-only `types.h`, `Holder { const Lay *lay; }`, `holderUse(Holder h)` | accepted | accepted; the non-`const` twin is rejected |
| M3: hand-written mirror, `outerFill(out: Outer \| null)`, no record | accepted (the reviewer's C AOT: `4 5 1`) | rejected: `Lay.items`; `const=true` record: accepted |

`cli/tests/read_records.rs` binds the headers with the binder and checks
the generated mirrors. Run in the pin tree with the pin binder, each of
its three tests fails (the pin accepts every form), so the M1 and M3
cases are Red. M2 is a defect of the round-4 tree, not of the pin.

Pin results of the reject entries at `7891a753`, with the interop
mirrors bound by the pin binder: r405–r413 are each accepted, with empty
stdout on each tier. r412 and r413 carry no parameter record: the
pointer counts as non-`const`.

| Test | Cost | What it proves |
|---|---|---|
| `cli/tests/read_records.rs` (3 tests) | 0.05 s; three headers per case, bound in memory | M1, M2, M3 end to end, each with a `const` twin |
| `compiler/tests/foreign_read_records.rs` (3 tests) | 0.01 s | value and `const` roots, fills, missing records fail closed, a handle by value with a struct control |
| `read_split_header::every_pointer_parameter_and_member_records_its_const` | one in-memory parse | parameter and member records, an undefined external included |

### Rule 2 amended: an embedded string view under a fill

`member_read` takes a `Reach` in place of a depth: `Root` (a direct
member of the read root), `Embedded` (reached through by-value embedding
only), or `Indirect` (reached through a struct-pointer member or pair
elements, or a root that starts there). `Reach::nested` keeps `Embedded`
through an embedded struct and gives `Indirect` through a pointer member
or a pair.

| Member | Copy root, any reach | Scratch, `Root` | Scratch, `Embedded` | Scratch, `Indirect` |
|---|---|---|---|---|
| bytes | readable | readable | readable | readable |
| pair, scalar elements | no read lowering | readable (rule 4) | no read lowering | no read lowering |
| pair, struct elements | elements, then no read lowering | `const`: readable; mutable: elements | elements, then no read lowering | elements, then no read lowering |
| string view | no read lowering | readable | readable (copy-back, §30.1) | no read lowering |
| callback | no read lowering | no read lowering | no read lowering | no read lowering |
| embedded descriptor aggregate | no read lowering | no read lowering | no read lowering | no read lowering |
| embedded struct | nested, same reach | nested, `Embedded` | nested, `Embedded` | nested, `Indirect` |
| struct-pointer member | nested, `Indirect` | nested, `Indirect`, then no read lowering | the same | the same |

A result is a copy root, so a string view in a result has no read
lowering at any reach; r409 and the result rows are unchanged and need
no new tier evidence. `abi_pressure::boundary_gate_in_both_tiers`
(`sweepNestedHalfWrite`) passes unchanged. The bindgen test
`recursive_read_direction_fails_loud_at_innermost_member` becomes
`recursive_embedded_string_view_has_a_fill_read_lowering` (accepted).
`read_lowering::tests::a_fill_reads_an_embedded_string_view_and_not_one_behind_a_pointer`
binds a depth-1 and a depth-2 embedded view under a fill (accepted) and
the same views behind a `const` pointer member (rejected at `Str.label`);
four in-memory parses.

Open (as 187.3):

1. A host that writes the direct pair of a filled out-parameter loses
   that write without a diagnostic (rule 4).
2. A by-value C array parameter binds as `FixedArray`, and C AOT fails
   to compile the call; when that is fixed, the scan must classify the
   position. The same mismatch holds for an array of struct pointers in
   a struct: `Out { int32_t k; P *ps[2]; }` binds as
   `ps: FixedArray<P, 2>` (by value), and C AOT stops at
   `_Static_assert(sizeof(Out) == sizeof(SubC2))` (`24 == 12`), measured
   in Phase Review 4.
3. A stale defining mirror: if a header changes a member or parameter
   to non-`const` and the mirror keeps `const=true`, the checker accepts
   the input. The byte-identical regeneration test catches it for
   committed mirrors only.

A record on a member or parameter that the predicate never reads (a
scalar pointee, a handle) has no effect.

## Phase Review 4 (2026-10-10)

Two MAJOR findings, both present before this section's diff, measured
in C AOT:

- `Holder { int32_t k; P *p; }` filled by `holderFill(Holder *out)`:
  the binder and the checker take a copy root; code generation builds a
  scratch struct (a `P | null` member is not byte-copyable) and the
  copy-back skips nullable members. C writes `out->p = &cp` (`cp.x ==
  77`); the run prints `5 -1`, expected `5 77`.
- `holderTouch(Holder h)` with `P { int32_t x; Q *q; }`: the target `P`
  goes to a scratch copy that is not written back; C writes
  `h.p->x = 9; h.p->q->y = 11`; the run prints `2 11`, expected `9 11`.
- `cmdRun(const Cmd *cmd)` with `Cmd { SubStringView name; Counter
  *counter; }` is rejected ("struct-pointer field of a scratch struct"),
  but code generation passes the script's `Counter`; with a `const`
  record the run prints `5`, which is correct.
- `fillA(A *out)` with `A { SubStringView name; void *ud; }`: C AOT
  writes `ud` back (`aa false`); the dev JIT skips every nullable member
  (`lower/func/boundary.rs`), so the tiers disagree.

Also measured: `Out { int32_t k; P *ps[2]; }` binds as
`ps: FixedArray<P, 2>`; C AOT stops at the `_Static_assert` on
`sizeof(Out)` (the 187.3 item 2 class).

Rule 3 now takes the read root from the code-generation fact; rule 6
requires the tiers to write back the same members.

### Fixes after Phase Review 4

#### The one pass decision

`boundary/src/pass.rs` decides how a call passes each struct and each
pointer. The boundary crate is the leaf crate: the binder, the compiler
(checker), and code generation each depend on it, and it depends on
none of them. Each consumer gives its structs through the trait
`StructView` (`fields(s) -> Option<Vec<FieldShape<S>>>`).

| Function | Signature | Decides |
|---|---|---|
| `struct_pass` | `fn struct_pass<V: StructView>(view: &V, s: V::Struct) -> StructPass` | `Bytes` (the call copies or shares the bytes) or `Scratch` |
| `parameter_pass` | `fn parameter_pass<V: StructView>(view: &V, target: V::Struct) -> PointerPass` | a pointer parameter: `ScriptMemory` or `ScratchWrittenBack` |
| `member_pass` | `fn member_pass<V: StructView>(view: &V, parent: StructPass, target: V::Struct) -> PointerPass` | a pointer member: `ScriptMemory` or `ScratchNotWrittenBack` |
| `element_pass` | `fn element_pass<V: StructView>(view: &V, element: V::Struct) -> PointerPass` | pair elements: the script array or a scratch array |
| `embedded_pass`, `value_parameter_pass`, `target_pass` | | the pass of an embedded struct, a by-value argument (always `Scratch`), and a pointer target |
| `copy_back` | `fn copy_back<S: Copy>(field: &FieldShape<S>) -> CopyBack<S>` | what the copy-back of a scratch struct writes (rule 6) |

Two views map a member to its `FieldShape`:

- `compiler/src/boundary_pass.rs`, `field_shape(classes, ty)`: one
  mapping from a language type. The checker (its mirror classes, with
  `apparent_type`) and both tiers (the LIR classes) use it. The LIR flag
  `copies_boundary_bytes` is `struct_pass` on the LIR view
  (`codegen/src/lir.rs`, `derive_boundary_byte_copy`).
  `is_embedded_header` moved here from `codegen/src/lir.rs`; the checker
  and code generation call it.
- `bindgen/src/read_lowering/view.rs`, `HeaderStructs`: the C field to
  the shape that its mirror type gives.

Call sites: the fill parameter (C AOT `marshal_foreign_value`, dev JIT
`marshal_foreign_argument`) calls `parameter_pass`; a pointer member of
a rebuilt struct (C AOT `marshal_boundary_struct`, dev JIT
`populate_boundary_value`) calls `member_pass`; pair elements (C AOT
`marshal_boundary_struct` and `call.rs`, dev JIT both array sites) call
`element_pass`; the copy-back (C AOT `emit_boundary_writeback`, dev JIT
`write_back_boundary_pointer`) matches `copy_back`. The read predicate
takes the passes: `MemberKind::of(view, parent, shape, mutable)` builds
`Embedded(StructPass)`, `StructPointer(PointerPass)`, and
`Pair { elements: Option<PointerPass> }`, and `member_read` returns the
nested root and reach. `written_member_read` decides a member of an
input struct. The binder's `boundary_aggregate_needs_scratch` and the
checker's `needs_scratch` are removed.

The binder's §28/§30 write validation (`emit.rs`, the scratch roots of
`validate_lowerable_boundary_aggregate`) keeps its scope:
`HeaderStructs::holds_view_or_pair` (a string view or a pair, by
embedding or behind a pointer member). It is not the pass: a struct
whose only lowered member is a callback is a scratch struct, and the
binder keeps it out of that validation, as before.

#### Decisions

Item 2 (`holderFill`, `holderTouch`): rejected at check. C can write a
pointer member of a filled scratch struct, and a C pointer cannot become
a script object, so the copy-back keeps the script link. A target that
the call passes as a scratch copy (`RrInner` of `holderTouch`) has a
pointer member of its own that C can write, so a write-back of the
target still leaves a member with no read lowering. `cmdRun` is
accepted: `RrCmd` is a scratch struct, and its `RrCounter` target copies
its bytes, so the call passes the script memory.

Item 3 (rule 6): a userdata slot of a scratch struct has no read
lowering, and neither tier writes it back. C can write any address
there, and a fill has no registration to validate it against, as the
callback trampoline validates its userdata. A handle member is written
back in both tiers. A byte-copied struct (a copy root) carries its C
bytes as they are in both tiers, userdata included.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Pin `7891a753` (pin binder, check, dev JIT, C AOT) | Now |
|---|---|---|
| `subReadRootHolderFill(RrHolder *out)`, C writes `out->p = &point` (`77`) | binds, check accepts; dev JIT `5 -1`, C AOT `5 -1` | binder and checker reject `RrHolder.p` |
| `subReadRootOuterTouch(RrOuter outer)`, C writes `inner->x = 9`, `inner->q->y = 11` | binds, check accepts; dev JIT `2 11`, C AOT `2 11` | binder and checker reject `RrInner.q` |
| `subReadRootNamedFill(RrNamed *out)`, C writes `name = "aa"`, `ud = &marker` | binds, check accepts; dev JIT `aa true`, C AOT `aa false` | binder and checker reject `RrNamed.ud` |
| `subReadRootCmdRun(const RrCmd *cmd)`, C adds `len` to `counter->n` | the binder rejects (`RrCmd.counter`) | dev JIT `5`, C AOT `5` |

No accepted corpus golden moves; the LIR text golden does not move; the
interop mirrors regenerate byte-identical.

#### MINOR items

- `callback_parameter_reads` reads a nullable callback field: it
  matches `Type::function_type()`.
- The `ForeignResultRead` `why` names every kind (24 words; the
  divergence test allows 25). Its `ts` form has a result, a fill with a
  pointer member, a userdata fill, a nested pair fill, pair elements,
  and a callback argument.
- `check_foreign_result_reads` documents the parameters and callback
  arguments that it scans.

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests` (3) | under 0.01 s | each pass at each site; the copy-back member set |
| `boundary` `read::tests` (5) | under 0.01 s | each kind at each root, reach, and pass; a written member; `MemberKind::of` takes its passes from `pass` |
| `compiler` `boundary_pass::tests` (3) | under 0.01 s, one in-memory check | each type to its shape; boundary value classes only; a checked chain header |
| `read_lowering::tests::boundary_passes_gives_each_struct_and_pointer_its_pass` | 0.01 s, one in-memory parse | the binder view |
| `cli/tests/read_root.rs` (5 tests) | 0.6 s for the file in parallel; alone: 0.56, 0.47, 0.48, 0.93 s for the four tests that run one or two programs in both tiers (one C compiler and one program process per C AOT run), 0.02 s for the pass comparison | each measured case end to end (bind, check, run) with a same-shape control; the binder, the checked classes, and code generation give one pass for each struct and pointer member |

Red at the pin: the pin tree (`git archive 7891a753`) with the fixture
files, run with the pin binder, fails each of the four behavior tests
of `read_root.rs` (the pin binder binds the three rejected forms and
rejects `subReadRootCmdRun`); the table above is its probe output. The
fifth test calls `boundary_passes`, which the pin does not have.


## Phase Review 5 (2026-10-10)

- MAJOR: a fill of `List { int32_t tag; size_t itemsCount; const Item
  *items; }` with `Item { int32_t k; P *p; }`, `P { int32_t x; Q *q; }`
  binds and checks; C writes `items[0].p->x = 9`; C AOT prints `5 2 11`,
  expected `5 9 11`. `member_read` returns readable for a `const` pair at
  the copy-back root and does not walk its elements.
- MAJOR (write direction, present before this section): `Holder { int32_t
  k; const Base *link; }` with `Base { int32_t kind; View name; }`
  embedded as the header of `Ext`: `copies_bytes` takes the
  embedded-header arm without checking that the header copies its bytes;
  `holderUse` gives `1002`, expected `1032`.
- MAJOR: `render(const Scene *scene)` with a non-`const` pointer graph is
  rejected. The owner decided on 2026-10-10 that pointer targets are
  written back (rule 7).
- MINOR: an embedded userdata slot under a fill is copied back by bytes
  in both tiers (`aa 9 false`) while a direct slot is rejected (rule 8
  now applies at every reach); `Unlowered` maps to bytes; the dev JIT
  copy-back of a `FixedArray<Struct, N>` member copies one element; the
  checker does not scan a foreign function's own callback parameter.
