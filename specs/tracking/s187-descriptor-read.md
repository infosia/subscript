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

| Entry | Form | Pin `ce01c7c7`: check, dev JIT, C AOT, interpreter | Now |
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
`ce01c7c7` source tree. The callback-field fill row is a binder unit
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
| CLI at `ce01c7c7` | binds | `42 5 1` (C wrote 3 items), `1 3 1` (C wrote tag 9 and 3 items), `false` |
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
| `typedef struct Outer Outer;` plus the external directive | bind error at `ce01c7c7` and now: the external is also defined in this header | the same |
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

Pin evidence at `ce01c7c7`: the pin copy takes this tree's `interop.h`
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

| Finding | At `ce01c7c7` with pin-binder mirrors | Now |
|---|---|---|
| M1: `c.h` defines `Lay`; `a.h` marks it external, `Outer { struct Lay *lay; }`; `b.h` includes `a.h`: `outerFill(Outer *out)`, `outerTouch(Outer o)`, `outerUse(const Outer *o)` | check accepts all three (the reviewer's C AOT: `42` / `1 3`) | rejected: `outerFill` reads `Lay.items`; `outerTouch` and `outerUse` through `Outer.lay` read `Lay.items`; with `const struct Lay *lay`, only the fill is rejected |
| M2: declaration-only `types.h`, `Holder { const Lay *lay; }`, `holderUse(Holder h)` | accepted | accepted; the non-`const` twin is rejected |
| M3: hand-written mirror, `outerFill(out: Outer \| null)`, no record | accepted (the reviewer's C AOT: `4 5 1`) | rejected: `Lay.items`; `const=true` record: accepted |

`cli/tests/read_records.rs` binds the headers with the binder and checks
the generated mirrors. Run in the pin tree with the pin binder, each of
its three tests fails (the pin accepts every form), so the M1 and M3
cases are Red. M2 is a defect of the round-4 tree, not of the pin.

Pin results of the reject entries at `ce01c7c7`, with the interop
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

| Form | Pin `ce01c7c7` (pin binder, check, dev JIT, C AOT) | Now |
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

Red at the pin: the pin tree (`git archive ce01c7c7`) with the fixture
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

### Fixes after Phase Review 5

#### Rule 7: the call writes back every scratch copy

`boundary/src/pass.rs` has no `ScratchNotWrittenBack` outcome. The
variant is removed. `member_pass` gives `ScratchWrittenBack` for every
target that builds a scratch struct, and `element_pass` gives it for
pair elements that build a scratch array. The copy-back of a pointer
member keeps the script link (187.3 item 4).

Both tiers keep one site list per foreign call (C AOT `BoundarySites` in
`codegen/src/cemit.rs`, dev JIT `BoundarySites` in
`codegen/src/lower/func.rs`). The list head is a local (C AOT) or a stack
slot (dev JIT), set to null before the marshalling. Each scratch copy of
a pointer target and each scratch array pushes one node from the
boundary scratch arena when the call builds it, also inside an element
loop. A node is five words:

| Word | Value |
|---|---|
| 0 | the next node |
| 1 | the site: an index into the list of classes of the call |
| 2 | the script pointer (the script element array for a pair) |
| 3 | the scratch pointer |
| 4 | the element count (1 for a pointer target) |

After the call, and before the scratch release, each tier walks the
list and applies `copy_back` to each element of each node, with the
language stride and the C stride of the site class. A pointer parameter
root keeps its direct write-back. A null script pointer gives a null
scratch pointer, and the copy-back skips it.

The string-view copy-back keeps the script string when the C view has
the data pointer and the length that the call gave it. It builds a new
string only for a view that C wrote. Without this rule, each call with a
read-only input rebuilt each string that the input holds. Measured: the
dev JIT `live_bytes` of `a163-address-taken-activation`
(`codegen/tests/lir.rs`) changes from 3025 to 1625. The 70 calls of the
entry each rebuilt the 20-byte copy of `"address-base"` (an 8-byte header
and 12 bytes): 3025 - 70 × 20 = 1625. The stdout golden does not move.

#### The one walk (rule 3)

`boundary/src/read.rs` has the trait `ReadView` (`name`, `members`) and
the one walk `first_unreadable(view, s, root, reach)`. It visits
embedded structs, pair elements, and pointer targets, and decides each
member with `member_read`. `ReadRoot` is `Value` (a result, a completion
result, a callback parameter), `Fill(StructPass)`, or
`Input(StructPass)`. `MemberKind::StructPointer { pass, mutable }`
carries the `const` fact of the member. `Unreadable::through` names the
outermost non-`const` member of an input through which C writes; the
message names it as `through`. A struct on the path at the same root and
reach ends the walk.

The binder (`HeaderStructs` in `bindgen/src/read_lowering/view.rs`) and
the checker (`Scan` in `compiler/src/check/foreign_read.rs`) implement
`ReadView` and call `first_unreadable`. Both `walk_written` functions
are removed. A `const` pair at a fill root walks its elements as an
input.

Behaviour change: a `const` pointer member under a fill is an input,
because C only reads its target, and the copy-back keeps the link.
Before, the fill read the target at the scratch root. Three tests change
for this: `bindgen/tests/provenance.rs`
`recursive_struct_pointer_read_direction_fails_loud_at_innermost_member`
(the `const` form binds, the non-`const` twin is rejected),
`cli/tests/read_records.rs` (the `const` twin is accepted), and the
binder unit test of rule 2 (its pointer cases use non-`const` members).

Rule 2 and rule 7 both apply to a string view behind a pointer: rule 7
writes the target back, and rule 2 says the view has no read lowering.
The predicate follows rule 2 and rejects it.

#### M2

`copies_bytes`, arm `FieldShape::Pointer { target, header }`: `header &&
(target is on the path || copies_bytes(target))`. A self-linked header
(`RrChain`) still copies its bytes.

#### Rule 8

At a `Fill` root, a userdata slot has no read lowering at every reach and
for both passes. The noun is "userdata field that C can write". A result
and a callback parameter read a userdata slot as before. No committed
corpus entry, example, or header depends on a C-written userdata slot.

#### MINOR items

- `FieldShape::Unlowered` is its own kind, `MemberKind::Unlowered`, with
  no read lowering at a value root and a fill root ("member with no call
  lowering"). The checker reads a member that holds a value class that
  is not a boundary struct as bytes. The binder reads a member whose type
  its parse does not define as bytes; the checker reads it.
- A `FixedArray` member of a struct that a call rebuilds is rejected in
  both tiers at code generation, with one text
  (`lower::fixed_array_member`). Before, C AOT failed at the C compiler
  (`((Out){ ((P*)(t7.d4)), t7.d5 })`) and the dev JIT failed in
  `boundary_c_field`, so the one-element dev JIT copy-back was not
  reachable. The binder and the checker do not reject the form.
- The checker scans each callback parameter of a foreign function as a
  value. The direct-callback mirror rule also rejects that parameter, so
  the form gives two diagnostics.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Now (dev JIT / C AOT) | Control, no call | Pin `ce01c7c7` (pin binder, dev JIT / C AOT) |
|---|---|---|---|
| `subReadRootOuterTouch(RrOuter outer)` | `9 11` / `9 11` | `2 3` | `2 11` / `2 11` |
| `subReadRootOuterBump(const RrOuter *outer)` | `12 13` / `12 13` | `2 3` | `2 13` / `2 13` |
| `subReadRootRender(const RrScene *scene)`, C adds 1 to `mesh->count` and doubles `mat->r` | `7 4 3` / `7 4 3` | `7 3 1.5` | `7 3 3` / `7 3 3` |
| `subReadRootItemListFill(RrItemList *out)`, `const RrItem *items` | `5 9 11` / `5 9 11` | `0 2 3` | the binder rejects (`RrInner.q`) |
| `subReadRootItemSpanTouch(RrItemSpan span)`, mutable scratch elements | `9 7` / `9 7` | `3 2` | the binder rejects (`RrInner.q`) |
| `subReadRootLinkedUse(const RrLinked *linked)`, header `RrBase` holds a view | `1032` / `1032` | `999` (no link) | `-2116511128` / `1002` |
| `subReadRootHolderFill(RrHolder *out)`, C replaces `out->p` | `5 70` / `5 70`: the link stays, the replacement is lost | `0 70` | `5 70` / `5 70` |
| `subReadRootNamedFill`, `subReadRootNamedUdFill` (embedded slot), `subReadRootUdFill` (bytes struct) | the binder and the checker reject `RrNamed.ud`, `RrUd.ud`, `RrUd.ud` | handle twin `bb 42` / `bb 42` | bind; `aa 9 false` (NamedUd), `9 false` (Ud) |

The reviewer probes give, in C AOT: `rv/i` `5 9 11`, `rv/l` `1032`,
`rv/d2` rejected (`Inner.ud`), `rv/h` binds.

Red at the pin: the pin tree (`git archive ce01c7c7`) with the fixture
files and `read_root.rs` without the pass-decision test fails 8 of its 9
tests. `a_fill_keeps_the_script_link_of_a_pointer_member` passes at the
pin: it records the 187.3 item 4 output. No accepted corpus golden and
no LIR text golden moves; the interop mirrors regenerate byte-identical.

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests` (3), `read::tests` (8) | under 0.01 s | each pass; the copy-back member set; the predicate at each root and reach; the walk over each route, `through`, a cycle; the messages |
| `bindgen` `read_lowering::tests` (6) | 0.1 s, in-memory parses | the binder walk at each root and depth; the scan of each position, with rule 8 in a scratch struct, a bytes struct, and an embedded struct; rule 7 behind a `const` and a non-`const` member |
| `compiler/tests/foreign_read_records.rs` (4) | 0.01 s | the checker records; a callback parameter of a foreign function |
| `cli/tests/read_root.rs` (9) | 2.4 s for the file in parallel; alone about 1.0 s for each accepted case (one program and its control, each in both tiers), 1.9 s for `a_pointer_target_is_written_back` (two forms), 0.55 s for the userdata case, 0.03 s for the pass comparison | each case end to end (bind, check, run in both tiers) with a control |

## Phase Review 6 (2026-10-10)

- MAJOR: the rule 7 write-back undoes a script write that a callback
  makes during the call. `QJob { QCallback callback; ...; QCount *count;
  }`, `qJobRun(const QJob *job)` calls the callback, which sets
  `job.count.n = 99`; C AOT prints `before 1`, `in hi 99`, `after 1`;
  expected `after 99`. The dev JIT has the same write-back (by reading).
  The copy-back also writes back `const` targets.
- MAJOR: `PNode { int32_t v; struct PNode *next; }` with
  `pNodeBump(PNode *node)` binds and checks; `subscript build` overflows
  the compiler stack (`marshal_boundary_pointer` and
  `marshal_boundary_struct` recurse without end).
- MAJOR: `FT { float m[4]; int32_t k; }` by value (`fSum(FT t)`) is an
  internal lowering error; `value_parameter_pass` always gives scratch.
- MINOR: one object passed to two fill parameters prints `11` (C gives
  `12`); the runtime cost of the site list is not measured; a mutable
  scalar pair is readable at the fill root and rejected at other
  reaches; the string keep check reads the current script field.

Rules 7, 9, 10, 11 and 187.3 item 7 answer these.

### Fixes after Phase Review 6

#### Rule 7: the copy-back writes the members that C changed

Each written-back scratch copy has a snapshot: a byte copy of the scratch
struct, taken after the call builds the struct and before the call. The
snapshot is in the same allocation, after the scratch struct (a pointer
target) or after the scratch elements (a pair). A site node has a sixth
word, the snapshot pointer. A pointer parameter root keeps its direct
write-back, with its own snapshot: in the C AOT allocation, and in a
second stack slot in the dev JIT.

The copy-back compares each member that `copy_back` names with its
snapshot bytes, and writes the member only when they differ. C AOT uses
`memcmp` over the C member. The dev JIT compares the C member bytes in
loads of eight, four, two, and one bytes. An embedded struct that copies
its bytes is one member; an embedded scratch struct is compared member by
member. A string view is compared as its view bytes (data pointer and
length). The copy-back no longer reads the current script string, so a
callback that replaces the string during the call keeps its value.

Memory cost: each written-back scratch copy takes twice its scratch bytes,
and each site node takes 48 bytes instead of 40. The boundary scratch
arena releases both at the end of the call. A `const` target and a
`const` pair take no snapshot and no site node.

#### Rule 7: the `const` fact reaches code generation

Route: the mirror records, through HIR and LIR. The checker attaches
`ForeignTypeProvenance::ConstPointer` (HIR and LIR) to each struct-pointer
parameter with a `const=true` `@subscript-c-parameter` record, and to each
struct-pointer member and pair member of structs with a `const=true`
`@subscript-c-member` record (`member_const_provenance`,
`foreign_parameter_provenance`). The LIR lowering copies it. The checker
scan reads the same HIR fact; `member_mutable` and the record lookup of
`parameter_reads` are removed. A pointer with no record has no
provenance and is non-`const`.

`parameter_pass`, `member_pass`, and `element_pass` take `writable`. A
`const` pointer to a scratch struct gives `PointerPass::ScratchReadOnly`:
the call builds the scratch copy and writes nothing back. Both tiers read
`writable` from the provenance at each site: the fill parameter, a pointer
member of a rebuilt struct, a pair member, and a by-value descriptor
(`element_const`).

#### Rule 9: a struct cycle

`struct_pass` gives `StructPass::Cycle` when the scratch build of a struct
reaches a struct that is already on the build path (`builds_cycle`); the
build follows each embedded struct, pointer target, and pair element
struct that does not copy its bytes. A pointer to such a struct is
`PointerPass::Cycle`. `first_cycle` walks the same edges through a
`ReadView` and names the member that closes the cycle;
`StructCycle::message` gives the text. The binder (`validate_parameter`)
and the checker (`parameter_read`) reject each parameter whose struct the
call builds and whose pass is a cycle, before the read walk. Code
generation stops with `lower::struct_cycle` (an internal error) at each
site that matches `Cycle`, in both tiers.

#### Rule 10

`value_parameter_pass(view, s)` is `struct_pass(view, s)`. C AOT passes a
struct that copies its bytes as `(*(T*)&tmp)`. The dev JIT passes the
script bytes with no scratch struct. `boundary_c_field` sizes a fixed
array, the leaf list has one leaf for each element, and
`boundary_c_layout` rejects a fixed array only in a struct that does not
copy its bytes. The unused `scratch_mark` parameter of the dev JIT
marshalling functions is removed.

#### Rule 11

`fill_read` gives `Readable` for a pair of scalar elements at every reach.
A pair of struct elements keeps its rule: at a reach other than the fill
root it has no read lowering, also when the element copies its bytes and
the call passes the script elements.

Rule 11 accepts four test forms and two reject entries of this section,
each with a scalar pair at an embedded or indirect reach. The forms now
use a pair of struct elements and keep their messages: the binder unit
test (`Outer`, `List`), `bindgen/tests/read_split_header.rs`,
`compiler/tests/foreign_read_records.rs`, and `cli/tests/read_records.rs`.
`r411` and `r412` were accepted by the checker. `SubDescReadMutHolder.inner`
in `corpus/interop/interop.h` now points to
`SGPUProbeVertexBufferLayout`, and `r412` embeds
`SGPUProbeVertexBufferLayout`; both reject
`SGPUProbeVertexBufferLayout.attributes`. The interop mirror regenerates
with one changed class. At the pin, with the pin binder's mirror of this
`interop.h`, both entries are accepted with empty stdout in the dev JIT,
C AOT, and the interpreter, as their headers state. The tsc header test
passes.

#### LIR text golden

`codegen/tests/lir-goldens/corpus.txt` moves by 270 lines: 264 lines
change `foreign=None` to `foreign=Some(ConstPointer)` on interop
parameters and members, and 6 lines change the target class of
`SubDescReadMutHolder.inner`. With `ConstPointer` text replaced by `None`
and the 6 lines excluded, the text is identical. No accepted corpus
golden moves.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Now (dev JIT / C AOT) | Round 7 tree (dev JIT / C AOT) | Pin `ce01c7c7` |
|---|---|---|---|
| `subReadRootJobRun(const RrJob *job)`: the callback sets `count.n = 99`, C leaves it | `run 99` / `run 99` | `run 1` / `run 1` | `run 99` / `run 99` (no write-back) |
| control `subReadRootJobSet`: callback, then C writes `count->n = 5` | `set 5` / `set 5` | `set 5` / `set 5` | `set 99` / `set 99` |
| `subReadRootRenderConst(const RrScene *scene)`: C writes `id = 50` through the `const` pointer, adds 1 to `mesh->count` | `50 7 4` / `50 7 4` | `50 50 4` / `50 50 4` | `50 50 3` / `50 50 3` |
| `subReadRootNodeBump(RrNode *node)`, `subReadRootCycleUse(const RrCycleA *a)` | the binder and the checker reject (`RrNode.next`, `RrCycleB.a`); the checker also rejects a by-value `RrNode` | the binder binds | the binder binds |
| `subReadRootFtSum(RrFt t)`, `RrFt { float m[4]; int32_t k; }` | `10` / `10` | internal error (fixed array in a rebuilt struct) | internal error (`boundary C field type FixedArray`) |
| control `subReadRootFtSumP(const RrFt *t)` | `10` / `10` | not run (the test stops at `subReadRootFtSum`) | not run |
| `subReadRootSceneDeform(const RrDScene *scene)`, `RrDMesh *mesh` with `float *verts` | `3 5` / `3 5` (control `1.5 2.5`) | the binder rejects (`RrDMesh.verts`, a count-first pair field) | the binder rejects (`RrDScene.mesh` may read `RrDMesh.verts`) |

The reviewer probes give, in C AOT: `p2/r` `before 1`, `in hi 99`,
`after 99`; `p3/f` `10`; `p1` (`pNodeBump`) rejected by the binder and by
the checker of the old mirror. `qTwoBump(t, t)` still prints `11`
(187.3 item 7).

#### Cost (release, Apple arm64, best of three)

The probe calls `subReadRootOuterTouch(RrOuter outer)` (a by-value scratch
struct with a written-back `RrInner` target) and
`subReadRootOuterBump(const RrOuter *outer)` in a loop of 20,000,000
calls. The per-call time is the run time less the run time of the same
program with no call, divided by the call count.

| Call | Pin `ce01c7c7` (dev JIT / C AOT) | Round 7 (site list) | Now (site list and compare) |
|---|---|---|---|
| `subReadRootOuterTouch` | 16.5 / 15.3 ns | 33.0 / 32.4 ns | 33.8 / 31.2 ns |
| `subReadRootOuterBump` | 16.5 / 27.7 ns | 33.2 / 45.5 ns | 33.8 / 46.2 ns |

The site list and the write-back of the target add about 17 ns per call
in both tiers. The snapshot and the compare add -1.2 to +0.8 ns per call;
a 5,000,000-call run gives the same differences. At the pin the call does
not write the target back.

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests` (3), `read::tests` (9) | under 0.01 s | the cycle pass for a self link, a mutual pair, a struct that reaches a cycle, and pair elements; `ScratchReadOnly` for each `const` site; rule 10; rule 11 at each reach; the cycle walk and its text |
| `bindgen` `read_lowering::tests` (6) | 0.1 s, in-memory parses | rule 11 in the predicate; the scan rejects a cycle at a pointer root and at a by-value root, each with a twin |
| `cli/tests/read_root.rs` (14) | 1.7 s for the file; alone 0.41 s (job), 0.69 s (`const`), 0.35 s (cycle), 0.63 s (fixed array), 0.66 s (scalar pair), 0.04 s (pass comparison) | each case bind, check, and run in both tiers with a control; the pass comparison covers `ScratchReadOnly` and reads the `const` fact from the LIR |

Red: the pin tree (`git archive ce01c7c7`) with the fixture files and
`read_root.rs` without the pass comparison fails the five new tests and
seven of the eight older tests; `a_fill_keeps_the_script_link_of_a_pointer_member`
passes, as before. The round 7 tree with the same files fails the five
new tests and passes the eight older tests. The table above is their
output.

## Phase Review 7 (2026-10-10)

- MAJOR: `nWalk(const NTop *top)` with `NTop { NMid *mids pair }`,
  `NMid { const NLeaf *leaves pair }` is rejected at `NMid.leaves`; the
  scalar twin `const int32_t *leaves` runs (`101 104`). Rule 11 now
  treats a pair of struct elements as a scalar pair when its elements
  are readable.
- MAJOR: `FR frGet(void)` with `FR { float m[4]; FP *p; }`: C AOT
  prints `3 42`; the dev JIT stops in `boundary_c_layout`, which rejects
  a fixed array when the write-direction pass is scratch (also at the
  pin). Rule 12 now lowers a result by its read facts.
- MINOR: a C-written header link becomes a script reference (`3 7`);
  the snapshot cannot order a C write against a callback write; the
  compare covers padding; the site list cost has no measured
  alternative; one rule 3 sentence had 35 words. 187.3 items 8-11
  record the first four.

### Fixes after Phase Review 7

#### Rule 11: a pair at every reach

`fill_read` reads a pair the same at every reach: a pair of scalar
elements is readable, and a pair of struct elements walks its elements
at the root that the element pass gives them. The `otherwise` pair kind at
an embedded or indirect reach is removed. A value root still has no read
lowering for a pair.

Rule 11 now accepts `r411`, `r412`, and the four test forms that round 8
moved to a pair of struct elements. Each of them tests a route of the
scan (an external struct, an include, a `const` record, an embedded
reach), not the pair, so each is retargeted to a member that stays
rejected:

- `SubDescReadMutHolder.inner` in `corpus/interop/interop.h` points to
  `SubDescReadStr`. `r411` reads `SubDescReadStr.label` through the
  mutable member (a string view behind a pointer, rule 2). `r412` embeds
  `SubDescReadMutHolder` under a fill and reads the same view; it is
  renamed `r412-foreign-fill-embedded-external-view` for its purpose. The
  interop mirror regenerates with one changed class; the corpus index
  regenerates.
- The binder unit test (`Outer`, `List`), `read_split_header.rs`,
  `foreign_read_records.rs`, and `read_records.rs` use a `void *ud` slot,
  which has no read lowering where C writes it (rule 8). The checker
  callback test keeps a pair in `Lay` for the value root. The binder
  unit test adds the accepted twin `ObjOuter` (a pair of struct elements
  under an embedded fill).

At the pin `ce01c7c7`, with the pin binder's mirror of this `interop.h`,
both entries are accepted with empty stdout in the dev JIT, C AOT, and
the interpreter, as their headers state. The tsc header test passes.

#### Rule 12: a result lowers by its read facts

The dev JIT has two C layouts: `CLayout::Write` for a struct that the call
builds or copies, and `CLayout::Read` for a result. A `Read` layout sizes
a fixed array in any struct; a `Write` layout rejects one in a struct that
does not copy its bytes, with the text of `fixed_array_member` ("a fixed
array in a struct that the call rebuilds"). `plan_foreign_struct_return`
uses the `Read` leaves. C AOT copies the result bytes and needs no change.

#### LIR text golden

The golden moves by 6 lines from round 8: the target class of
`SubDescReadMutHolder.inner`. No accepted corpus golden moves.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Now (dev JIT / C AOT) | Pin `ce01c7c7` |
|---|---|---|
| `subReadRootNWalk(const RrNTop *top)`, mutable pair of `RrNMid`, each with a `const` pair of `RrNLeaf` | `101 104` / `101 104` | the binder rejects (`RrNTop.mids` may read `RrNMid.leaves`) |
| control `subReadRootSWalk`, the scalar twin | `101 104` / `101 104` | not run (the bind of the test header fails) |
| `RrFr subReadRootFrGet(void)`, `RrFr { float m[4]; RrFp *p; }` | `3 42` / `3 42` | dev JIT: internal error (`boundary C field type FixedArray(F32, 4)`) |
| control `RrFb subReadRootFbGet(void)`, `RrFb { float m[4]; }` | `3` / `3` | not run (the dev JIT stops on the program) |

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `read::tests` (9) | under 0.01 s | rule 11 for each pair kind at each reach |
| `cli/tests/read_root.rs` (16) | 1.7 s for the file; alone 0.72 s (nested pairs), 0.37 s (results) | each case bind, check, and run in both tiers with a same-shape control |

Red: the pin tree (`git archive ce01c7c7`) with the fixture files and
`read_root.rs` without the pass comparison fails both new tests, and 12
of the 13 older tests; `a_fill_keeps_the_script_link_of_a_pointer_member`
passes, as before.

## Phase Review 8 (2026-10-10)

- MAJOR: `render(const Scene *s)` with `Mesh { float bounds[4];
  Material *mat; }`: bind and check pass; `subscript build` stops with
  "boundary field 'Mesh.bounds' is a fixed array in a struct that the
  call rebuilds" (both tiers by code). Rule 13 now copies a scalar fixed
  array as bytes.
- MAJOR: a C-writable pair of `CEnum` elements (`ModeC *modes`) is
  readable; C writes `12345`, the script shows `a` with no trap, while
  the struct member `r.mode` traps (t49). Rule 11 now gives such a pair
  no read lowering.
- MINOR: rule 8 rejected a C-written userdata slot under a fill and read
  it in a result; rule 8 now covers results. Doc, cfg, and STE items;
  a mutable pair of string-view elements is the 187.3 item 5 class.

### Fixes after Phase Review 8

#### Rule 13: a scalar fixed array is bytes

`lower::is_scalar_fixed_array` is true for a fixed array whose shape is
`FieldShape::Bytes` (scalar elements). In a scratch struct:

- C AOT builds the struct with `{0}` in the array position, stores it in
  a temporary, and copies the script array into it with `memcpy`. The
  copy-back compares the member with its snapshot and copies it back with
  `memcpy`.
- The dev JIT copies the array bytes in `populate_boundary_value` and in
  the copy-back. `boundary_c_layout_in` (`Write`) rejects only a fixed
  array of structs in a struct that does not copy its bytes.

187.3 item 6 stays: a fixed array of structs in a struct that a call
rebuilds stops in both tiers. Its text now says "a fixed array of
structs".

#### Rule 11: a writable pair of validated elements

`FieldShape::ValidatedPair` is a pair of `CEnum` elements (§52): the
binder view gives it for a pair whose element kind is `Kind::CEnum`, and
the compiler view for an array of a `StringAlias` element. It copies no
bytes and the copy-back skips it. `MemberKind::ValidatedPair { mutable }`
has no read lowering at a fill and at an input when it is mutable
(`UnreadableKind::ValidatedPair`, "pair of validated elements that C can
write"), and is readable when it is `const`. A value cannot read it.

The binder emits a `@subscript-c-member` record for each pair of `CEnum`
elements, so the checker has its `const` fact. The wire-enum mirror
regenerates with one new record:
`aggregate="SubWireModeRecord" member="modes" const=true`.

A by-value descriptor of mutable `CEnum` elements (`ModeSpan { ModeC
*items; size_t count; }`, `spanFill(ModeSpan span)`) is the same class.
The binder now rejects it at `ModeSpan.items`. The checker accepts a
mirror that declares it: the descriptor record names the aggregate and
the element type, not the member, so the checker cannot give the shared
text. A struct-member pair and a parameter pair (the binder rejects a
`CEnum` parameter pair) are closed.

#### Rule 8: a result

`value_read` gives a userdata slot no read lowering. No committed corpus
entry, example, or header reads a userdata slot from a result: every
suite passes.

#### MINOR items

- `member_const_provenance` and `foreign_parameter_provenance` each have
  their own doc comment.
- The `ForeignResultRead` `why` names the member kinds of this section
  and the struct cycle (25 words).
- `READ_ROOT_DIRECTORY` and `READ_ROOT_HEADER` have the `cfg` of
  `cli/tests/read_root.rs`.
- The `cli/tests/read_root.rs` header names rules 3 and 6 to 13 and the
  measured cost.

No accepted corpus golden and no LIR text golden moves.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Now (dev JIT / C AOT) | Pin `ce01c7c7` |
|---|---|---|
| `subReadRootBoundsRender(const RrBScene *scene)`, `RrBMesh { float bounds[4]; RrMaterial *mat; }`: returns `bounds[0] + bounds[3]`, writes `bounds[1] = 20`, doubles `mat->r` | `5 20 3` / `5 20 3` (control, no call: `-1 2 1.5`) | dev JIT: internal error (`boundary C field type FixedArray`) |
| `RrTr { float m[4]; RrFp *p; }`: `subReadRootTrSum(const RrTr *)`, `subReadRootTrVal(RrTr)` | `10 10` / `10 10` | not run (the test stops at the first program) |
| `subReadRootTrFill(RrTr *t)` writes `m[2] = 30` | `30` / `30` (control `3`) | not run |
| `subReadRootRecFill(RrRec *r)`, `subReadRootRecTouch(RrRec r)`, `RrModeC *modes` | the binder and the checker reject `RrRec.modes` | the binder binds |
| `const` twin `subReadRootRecCGet(const RrRecC *r)` | `2 b` / `2 b` | not run (the test stops at the first bind) |

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `read::tests` (10) | under 0.01 s | rule 8 at a value; rule 11 for a validated pair at each root, reach, and pass |
| `bindgen` `read_lowering::tests` (6) | 0.1 s | rule 8 in a result at each depth |
| `cli/tests/read_root.rs` (18) | 2.0 s for the file; alone 1.42 s (fixed arrays: two programs, each in both tiers) and 0.38 s (validated pair) | each case bind, check, and run in both tiers with a control |

Red: the pin tree (`git archive ce01c7c7`) with the fixture files and
`read_root.rs` without the pass comparison fails both new tests, and 14
of the 15 older tests; `a_fill_keeps_the_script_link_of_a_pointer_member`
passes, as before.

## Phase Review 9 (2026-10-10)

- MAJOR: an embedded-header link passes the header class only. With
  `PExt { PBase base; PView name; }` and `PHolder { const PBase *link;
  }`, C AOT returns `10000` (expected `10003`); with the view in the
  header, the scratch allocation holds one `PBase` and C reads `extra`
  past it (`3`, expected `7703`). Rule 14 answers it.
- MAJOR: the binder's §28/§30 validation rejects a scalar fixed array
  beside a pair or a view (`PMesh`), although the hand-edited mirror
  runs (`528`). Rule 15 answers it.
- Owner decisions of 2026-10-10: rule 8 accepts userdata and callback
  fields of a scratch struct (C writes lost); rule 9 passes a `const`
  cycle of byte-copied structs as script memory.
- MINOR: STE sentence length in the contract; a C member named `in`
  breaks the mirror constructor (present before this section).

### Fixes after Phase Review 9

#### The pass decision (rules 9 and 14)

`struct_pass` decides from the largest set of structs that copy their
bytes (`largest_copying_set`), over the structs that `s` reaches. A member
copies when it is bytes, a userdata slot, an embedded struct in the set, a
link to an embedded header whose family is in the set (rule 14), or a
`const` pointer to a struct in the set (rule 9). `FieldShape::Pointer`
carries `constant`. The decision for a struct reads only the structs it
reaches, so `copying_structs` decides every struct of a view at once; the
LIR lowering uses it for `copies_boundary_bytes`.

- Rule 9: `PNode { int32_t v; const struct PNode *next; }` copies its
  bytes and passes as script memory. The non-`const` list stays a cycle.
- Rule 14: `header_family_outlier` gives the first struct of the family of
  a link target (the target and every extension, at any depth) that does
  not copy its bytes. `parameter_pass` and `member_pass` give
  `PointerPass::HeaderLink` for such a link. `first_header_link` finds the
  link at a position and below it; `HeaderLink::message` is the shared
  text. The binder (`validate_parameter`) and the checker
  (`parameter_read`) reject each position before the cycle and read
  checks. Both code-generation tiers stop with `lower::unbuildable`.

How each view gives the extensions (`StructView::extensions`, the boundary
value classes whose first member is the target by value, not in a fixed
array):

| View | Source |
|---|---|
| Binder (`HeaderStructs`) | the parse: a boundary struct whose first field is not a pointer, not an array, and has the target as its base |
| Checker (`Scan`) | the mirror classes, once per check: a boundary value class whose first field has the apparent type `Class(target)` |
| Code generation and the checked module (`Classes<lir::Module>`, `Classes<hir::Module>`) | `BoundaryClasses::extensions`: a boundary value class whose first field type is `Class(target)` |

The checker `Scan` reads each class once (`ScanFacts`): the fields, the
read members, and the extensions.

#### Rule 8 (owner decision)

`fill_read` reads a userdata slot and a callback field of a scratch fill
(`Fill(Scratch)`). In a fill whose bytes the call passes (`Fill(Bytes)`,
also a struct that copies its bytes embedded in a scratch struct) the
userdata slot has no read lowering. A value reads neither.

`pStep(PWorld *w)` with `PBody { float x, y; void *user; }` stays
rejected. `PBody` copies its bytes, so `member_pass` passes its script
memory, and C writes `user` there: the position is the "struct whose bytes
the call passes as script memory" of rule 8. The binder gives
`foreign function `subReadRootPStep` parameter `w` reads `RrPBody.user`, a
userdata field that C can write`. The handoff expected it accepted; the
contract and the case disagree.

#### Rule 15

The binder's §28/§30 write validation rejects a fixed array only of
aggregates (a boundary struct, a string view, a descriptor, or a callback)
and no callback field. It no longer rejects a struct-pointer or pair
element type cycle: the pass decision gives the cycle outcome, and the
walk visits each struct once. The descriptor-aggregate rejection and the
`holds_view_or_pair` scope remain, because no pass lowers an embedded
descriptor aggregate.

#### Corpus and tests that rules 8 and 9 accept

- `r413` read a callback field of an embedded external struct under a
  fill; rule 8 accepts it. It is now
  `r413-foreign-fill-embedded-external-userdata`: it embeds
  `SubDescReadUd { int32_t k; void *ud; }`, which copies its bytes, from
  `corpus/interop/external-device.h` (the external-device mirror is not
  in the LIR text golden). The external-device mirror regenerates with one
  new class. At the pin `ce01c7c7`, with the pin binder's mirrors, the
  checker accepts it; the dev JIT and the interpreter print nothing; C AOT
  fails at the C compiler (the layout assertion names `SubReadInfoBox`,
  which no included header declares).
- The binder unit tests, `bindgen/tests/provenance.rs`,
  `bindgen/tests/read_split_header.rs`, and
  `compiler/tests/foreign_read_records.rs` move each callback or scratch
  userdata case to a userdata slot in a struct that copies its bytes, to a
  result, or to the callback shape rule. The cycle test expects the rule 9
  text.

No accepted corpus golden and no LIR text golden moves.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Form | Now (dev JIT / C AOT) | Pin `ce01c7c7` |
|---|---|---|
| `subReadRootHUse(const RrHHolder *)`, extension `RrHExt` holds a view | the binder and the checker reject `RrHHolder.link` (`RrHExt`) | not run (the test stops at the first bind) |
| `subReadRootLinkedUse`, header `RrBase` holds a view | the binder rejects `RrLinked.link` (`RrBase`) | binds |
| control `subReadRootGUse`, `RrGBase`/`RrGExt { RrGBase base; int32_t extra; }` | `7701` through `e.base`, `5` for a plain base | not run |
| `subReadRootPDraw(const RrPMesh *)`, `subReadRootPDrawV(RrPMesh)`, `subReadRootPNamedUse(const RrPNamed *)` | `528 528 304` / `528 528 304` | the binder rejects `RrPNamed.m` |
| `subReadRootNamedFill(RrNamed *)`, C writes `name` and `ud` | `aa true` / `aa true` (control `x true`) | not run |
| `subReadRootPSubmit(RrPJob *)`, C writes `prio = 9` and `userdata` | `9 true` / `9 true` (control `1 true`) | not run |
| `subReadRootPStep(RrPWorld *)` | the binder rejects `RrPBody.user` | not run |
| `subReadRootCSum(const RrCNode *)`, 3 nodes | `6` / `6` (one node: `3`) | the process stops (exit 132, SIGILL) |

#### Cost

The LIR text golden test (`coroutine_and_measurement_lir_text_matches_goldens`)
takes 3.7 s; it took 3.3 s in round 10. The first version of the decision
took 23.8 s: it read every class for each struct. The view method
`extensions`, the header flag, and the facts that `Scan` reads once bring
it back. A check of the interop mirror with an empty program takes 27 ms;
8 ms of it is the read scan.

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests` (4), `read::tests` (11) | under 0.01 s | rule 9 `const` cycle, rule 14 family and link walk, `copying_structs` equals `struct_pass` |
| `cli/tests/read_root.rs` (20) | 3.2 s for the file; alone 1.53 s (header links), 1.77 s (userdata), 0.39 s (fixed array beside a pair or a view), 0.67 s (`const` cycle) | each case bind, check, and run in both tiers with a control |

Red: in the pin tree (`git archive ce01c7c7`) with the fixture files and
`read_root.rs` without the pass comparison, the four new tests fail when
each runs alone; the `const` cycle test stops the test process (exit 132),
so the whole file stops at the pin.

## Phase Review 10 (2026-10-10)

- MAJOR: rule 14 rejected every pointer to a header struct when any
  extension does not copy its bytes: `cross(const Vec3*, const Vec3*,
  Vec3 *out)` beside `Entity { Vec3 pos; Mesh *mesh; }`, `getMousePos(Vec2
  *out)` beside `Label { Vec2 pos; const char *text; }`, and a reduced
  raylib header (`DrawMeshInstanced(Mesh, const Matrix *transforms,
  int32_t)`) were rejected; across two mirrors the checker rejected the
  whole program. `gUse(e.base)` passes an extension box through a
  pointer parameter (C AOT `7702`). The owner decided on 2026-10-10 that
  the call dispatches on the box class id (rule 14).
- MINOR: a header-link member ignored the `const` condition of rule 9
  (an unrelated `Tagged { Node n; }` made a non-`const` list accepted);
  the binder cost grows about cubically with reach depth (N=100 chain:
  20 s debug, 2 s release); the `why` text does not name header links or
  cycles; two checker views resolve a first field differently for a
  hand-written alias. Present before this section: `P *ps[2]` binds as
  `FixedArray<P, 2>` (C AOT `_Static_assert` "8 == 4").

### Fixes after Phase Review 10

#### Rule 14: dispatch on the class id of the box

`PointerPass::HeaderLink` (a rejection) is now `PointerPass::Dispatch {
writable }`: the outcome for a link to an embedded header whose family
has a struct that does not copy its bytes. The binder and the checker no
longer reject it (`first_header_link` and `HeaderLink` are removed). The
read walk reads every struct of the family, each at the root of its own
pass (`link_roots`), and the cycle walk follows every struct of the family
(`builds_cycle`, `first_cycle`). `class_pass` gives the pass of one known
class.

Both tiers read the class id of the box at `CLASS_ID_OFFSET` (-8) and
branch on the extensions of the family; the header is the default. Each
branch passes its class by `class_pass`: the script memory of a class that
copies its bytes, else a scratch copy of that class, written back through
the site list when the pointer is not `const`. C AOT emits a `switch`
(`marshal_boundary_dispatch`); the dev JIT emits a compare chain
(`dispatch_boundary_pointer`). A class id outside the family passes as the
header.

The class id exists only on a box. A foreign argument whose LIR operand is
an address of a script value (`l::ValueType::Address`, a local or a
temporary) has no box header, so both tiers pass it by the class of the
address and do not dispatch. A box that holds a C pointer (187.3 item 8)
has no class id either; a dispatched call reads the 4 bytes before it.

`boundary_type_requires_build` opens a scratch scope for a dispatched
parameter.

#### Rule 9 const condition for links (not applied)

`member_copies` with the `const` condition for a link (`header: true`)
makes `SubChainHeader { SubChainKind sType; struct SubChainHeader *next; }`
of `corpus/interop/interop.h` a cycle. The checker then rejects every
foreign function that passes it (`subChainPayloadValue`,
`subDeviceCreate`), so `a25`, `a30`, `a89`, and `a169`, and every program
that loads the interop mirror, fail. The change is reverted: a link copies
as it is when its family copies its bytes, `const` or not. The
`Node`/`Tagged` case binds.

#### Binder and checker cost (core principle 15)

`HeaderStructs` reads each struct once (`StructFacts`: fields, members,
extensions) and decides the copying set and the cycle set once
(`StructView::copying`, `StructView::cycles`, `copying_structs`,
`cycle_structs`). `validate` and the §28 validation build one view for
every position. The checker `Scan` keeps the same two sets.

| Synthetic chain (`b50.h`, `b100.h` of the review) | Before | After |
|---|---|---|
| `subscript bind`, N=50, debug / release | 3.09 / 0.65 s | 0.08 / 0.01 s |
| `subscript bind`, N=100, debug / release | 18.97 / 1.87 s | 0.29 / 0.04 s |
| `subscript check` of the N=100 mirror, debug / release | 2.91 / 0.32 s | 0.10 / 0.01 s |

(The check input renames the functions `f0`…`f99` to `fn0`…: a function
named `f32` hides the type.)

#### Own site for a cycle

`RejectionSite::ForeignStructCycle` (S100, `Divergence::ForeignStructCycle`)
reports a struct cycle; its `why` is "A call builds each scratch copy once;
a struct that reaches itself through pointers it must copy has no finite
scratch build (§187 rule 9)." The `ForeignResultRead` `why` names the read
members only. The rejection witness `foreign-struct-cycle` is registered.
A header link no longer rejects, so it has no site.

#### One first-field comparison

`boundary_pass::extends` compares the first field with `Class(id)` exactly.
`is_embedded_header`, the HIR and LIR extensions, the default
`BoundaryClasses::extensions`, and the checker's `MirrorClasses::extensions`
call it. The binder compares the C base name of a by-value first field.

#### Recorded, not fixed

`P *ps[2]` binds as `FixedArray<P, 2>`; C AOT fails at `_Static_assert`
(`8 == 4`). It is present at the pin, beside 187.3 item 2.

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Case | Now (dev JIT / C AOT) |
|---|---|
| `subReadRootCross(e.pos, Vec3(0,1,0), out)`, `RrEntity { RrVec3 pos; RrVMesh *mesh; }`; plain `Vec3` boxes | `1 -1` (`out.z`, `plain.z`) |
| the same across two mirrors (`RrEntity` declares `RrVec3` external) | `1` |
| `subReadRootMousePos(v)`, `subReadRootMousePos(label.pos)`, `RrLabel { RrVec2 pos; RrView text; }` | `3 4 4` |
| `subReadRootDrawMeshInstanced(mesh, model.transform, 1)`, `subReadRootDrawMeshInstanced(mesh, matrix, 1)`, `RrModel { RrMatrix transform; int32_t meshTotal; RrRMesh *meshes; }` | `371 351` |
| `subReadRootHUse`, extension `RrHExt` holds a view | `10003` (plain base `5`) |
| `subReadRootLinkedExt`, header `RrBase` holds a view, extension `RrExt` | `7703` (plain base `5004`) |
| `subReadRootWTouch` writes `RrWExt.extra` through a non-`const` link, `subReadRootWRead` reads it | `1077 77` (written back; control not touched) |
| `subReadRootGUse`, a family that copies its bytes (script memory) | `7701`, `5` |

The review's `Label { Vec2 pos; const char *text; }` does not bind: plain
`char` has no mapping (present before this section). The fixture uses a
string view for the text. The raylib `Model { int32_t meshCount; Mesh
*meshes; }` does not bind: the names form a count/pointer shape with no
collapse (present before this section); the fixture names the count
`meshTotal`.

Dispatch cost (release, best of three, 20,000,000 calls of
`subReadRootCross` with three boxes): with no extension of `RrVec3` the
call takes 4.2 ns (dev JIT) / 1.6 ns (C AOT); with `RrEntity` declared it
takes 4.1 / 3.3 ns. The C AOT difference is the scratch scope and the
three `switch` statements. A pointer to a struct with no extension emits
no dispatch.

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests` (4), `read::tests` (11) | under 0.01 s | dispatch outcome; `link_roots`; the walk reads an extension's member; the `const` cycle and `CTagged` |
| `cli/tests/read_root.rs` (21) | 3.3 s for the file; alone 2.63 s (class id dispatch, six programs) and 0.81 s (math and graphics headers) | each case bind, check, and run in both tiers with a plain-header control |

Red: at the pin `ce01c7c7` (a `git archive` copy with the fixture files),
`a_link_to_an_embedded_header_dispatches_on_the_class_of_the_box` fails:
the dev JIT prints `1761455056` and C AOT prints `10000` for `10003`.
`a_header_of_a_math_or_graphics_header_binds_and_runs` passes at the pin:
the pin passes each link as the script memory of the box. It failed in the
round 11 tree, where the binder and the checker rejected these headers
(Phase Review 10).

## Phase Review 11 (2026-10-10)

- MAJOR: a foreign pointer result is C memory with no box header, and
  the call dispatched on it. `IV2 *ivAt(int32_t i)` returns `&grid[i]`;
  `ivSum(ivAt(1))` printed `707` until `grid[0].x` equalled the class id
  of `Lab`, then the process stopped (C AOT, exit 139). At `Tile`'s id it
  wrote a 24-byte `Tile` copy back into an 8-byte C object. Rule 14 now
  asks the runtime whether the address is a Context allocation first.
- MAJOR: `Node { Vec3 pos; struct Node *parent; }` made every `Vec3*`
  function a rule 9 cycle, also across mirrors. The owner decided on
  2026-10-10: the call traps only when the box has the cycle class.
- MINOR: an unrelated extension with a view makes a writable header link
  reject (`H2 { IV2 *link; }` with `Lab { IV2 pos; View text; }`); the
  §28 bind rejection "not passed through any foreign pointer parameter"
  is false under rule 14; a two-level projection boxes the inner value;
  `ivBump(p.pos)` on a local value loses C's write (`1`, C wrote `2`);
  `const kt: Tile | null = keep.t; new H2(3, kt.pos)` is an internal
  error at `codegen/src/lir/place.rs:502`.

### Fixes after Phase Review 11

Scope (owner, 2026-10-10): items 1 and 2 of the review are fixed; items 3
to 7 are recorded below.

#### Rule 14: C memory passes as it is

Before the class-id read, both tiers call
`subscript_rt_owns_payload(ctx, payload)` (`runtime/src/ffi/boundary.rs`).
It answers `Context::owns_payload`, which is `live_payload_size(payload)
.is_some()`: a hash lookup of the payload address in the dev tier, and in
the ship tier a binary search of the chunk table (`arena_lookup_block`)
with the block grid and bump checks, then the large-allocation map. An
address that is not the payload of a live allocation (C memory, or an
address inside an allocation) answers 0, and the call passes the pointer
as it is: no class-id read, no scratch.

#### Rule 14: a cycle class traps

`pointer_to` gives `PointerPass::Dispatch` for every link whose family
does not copy its bytes; a family member that is a rule 9 cycle no longer
makes the link a cycle. `builds_cycle` follows a link into a family member
only when that member is not a cycle on its own (`evaluating` ends the
decision), and `first_cycle` and `link_roots` skip such a member. The
binder and the checker search a cycle only for a parameter that does not
dispatch.

When the box has a cycle class, both tiers call
`subscript_rt_trap_boundary_class(ctx, class, function, pos)`. The trap
kind is `class-mismatch` (`TrapKind::ClassMismatch`), with the message
"foreign function `F` received a box of class `C`, which reaches itself
through scratch copies; no call can build it (compiler.md §187 rule 14)".

#### Measured (Apple arm64, fixture `codegen/tests/native-fixture/read-root.h`)

| Case | Now (dev JIT / C AOT) |
|---|---|
| `subReadRootIvSum(subReadRootIvAt(1))` with `grid[0].x` set to 0…63 (every small class id), `RrLab` and `RrTile` declared | `707` for every `k` in both tiers |
| `subReadRootIvBump(subReadRootIvAt(1))`, then the sum | `807`: C memory written in place |
| control: a script `RrIV2` box bumped, an `RrTile` box (`t.pos`) | `807`, `102` |
| `subReadRootCross` beside `RrNodeV { RrVec3 pos; struct RrNodeV *parent; }`, plain boxes | binds, checks, `1`; across two mirrors it checks |
| `subReadRootCross(new RrNodeV(...).pos, b, out)` | both tiers trap: `class-mismatch`, the message above |

Cost (release, best of three, 20,000,000 calls of `subReadRootCross` with
three boxes read from an array, which makes each argument a box):

| Header | dev JIT | C AOT |
|---|---|---|
| no extension (no dispatch) | 5.9 ns | 1.6 ns |
| `RrEntity` declared, dispatch without the membership check | 7.9 ns | 4.8 ns |
| dispatch with the membership check | 31.2 ns | 16.7 ns |

The check costs about 7.8 ns (dev tier: a hash lookup) and 4.0 ns (ship
tier: the chunk binary search) for each dispatched link. A pointer to a
struct with no extension emits no dispatch and no check. A local value
passes as an address with its static class, so its call has no dispatch
either.

#### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests::a_cycle_class_in_a_family_dispatches` | under 0.01 s | the cycle class does not make the header or a holder a cycle |
| `a_dispatched_link_passes_c_memory_as_it_is` | 1.13 s | item 1 with script-box controls |
| `a_box_of_a_cycle_class_traps` | 0.77 s | item 2, one mirror and two |

`cli/tests/read_root.rs` (23 tests) runs in 3.8 s. Red: at the pin
`ce01c7c7` (a `git archive` copy with the fixture files),
`a_box_of_a_cycle_class_traps` fails (the dev JIT prints `1` with no
trap). `a_dispatched_link_passes_c_memory_as_it_is` passes at the pin: the
pin passes every link as script memory and never reads a class id. It
guards the round 12 dispatch, which the review measured failing (exit 139).

#### Recorded, not fixed (Phase Review 11 items 3 to 7)

3. An unrelated extension with a view (`Lab { IV2 pos; View text; }`)
   makes a writable header link reject (`H2 { int32_t k; IV2 *link; }`,
   `h2Touch(H2 *h)`), although the dispatch builds `Lab` only for a `Lab`
   box. A per-class read predicate needs the read walk in code
   generation (a `ReadView` of the LIR classes); it does not fall out of
   the cycle trap.
4. The §28 bind rejection "mirror-visible but the struct is not passed
   through any foreign pointer parameter" is false when a rule 14
   dispatch can build the struct.
5. `ivBump(p.pos)` with a local value `p` boxes a copy of `p`; C's write
   reaches the box, and `p` prints `1` (C wrote `2`). `ivBump(p.size)`
   passes the field address.
6. `const kt: Tile | null = keep.t; if (kt !== null) new H2(3, kt.pos)`
   is an internal error at `codegen/src/lir/place.rs:502` (the narrowed
   value has a nullable LIR type where the projection expects the class).
7. A two-level projection (`new Tile2(new Tile(...), 7).base.pos`) boxes
   the inner value, not the outer extension.

## Cost against the pin

The pin is `85c2fbcf`. The tree is the working tree on `ce01c7c7`. Both
are copies outside the repository, built `--release` with the same
toolchain. Apple arm64, AC power, one build or run at a time.

Method: one fixture header pair and one C file. A program times a loop
of N calls with a scalar foreign clock (`CLOCK_MONOTONIC_RAW`) and
prints the time and a result. The per-call time is the loop time
divided by N, and includes the loop (an empty loop takes 0.6 ns in the
dev JIT and 0.1 ns in C AOT). Each value is the best of three
processes. N is 20,000,000 for items 1 to 6, and 200,000 for item 7.
Both copies print the same results, except where the table says.

Header `a`: `PVec3 { float x, y, z; }` with no extension. Header `b`:
the same `PVec3`, with `PEntity { PVec3 pos; PMesh *mesh; }`. Argument
forms: "local" is `const a = new PVec3(...)`; "elem" is an element of a
`PVec3[]`; "box" is an element of a `(PVec3 | null)[]`; "cmem" is the
result of `PVec3 *pAt(int32_t)` (C memory); "ext" is `ent.pos` of a
local `PEntity`.

### Per call (ns; dev JIT pin / tree, C AOT pin / tree)

| Item | Call | dev JIT | C AOT | Cause in the tree |
|---|---|---|---|---|
| 1 | `pAdd(i, 1)` | 1.95 / 2.00 | 1.35 / 1.38 | none: the emitted C is the same |
| 2 | `pVecSum(v)`, by value | 4.05 / 3.87 | 1.47 / 1.05 | rule 10: the call passes the bytes (`*(PVec3*)&t`), not a compound literal |
| 3 | `pDot`, header `a`: local, elem, box, cmem | 3.96 / 4.05, 4.62 / 4.70, 5.25 / 5.01, 4.03 / 3.83 | 1.32 / 1.38, 1.46 / 1.38, 1.87 / 1.38, 1.52 / 0.96 | none: the emitted C is the same |
| 3 | `pScaleInto(a, out)`, header `a`: local, elem, box, cmem | 4.05 / 3.96, 4.64 / 4.47, 5.14 / 5.14, 3.73 / 3.93 | 1.51 / 1.11, 1.48 / 1.11, 1.85 / 1.89, 0.93 / 1.27 | none |
| 4 | `pDot`, header `b`: local, elem | 3.98 / 3.74, 4.39 / 4.42 | 1.35 / 2.93, 0.99 / 3.07 | rule 14: a scratch mark and release around the call; no dispatch |
| 4 | `pDot`, header `b`: box | 5.15 / 21.66 | 2.13 / 10.80 | rule 14: `subscript_rt_owns_payload` and the class-id switch, for each of two links |
| 4 | `pDot`, header `b`: cmem | 4.05 / 9.41 | 1.46 / 10.22 | rule 14: the same; the runtime answers 0 |
| 4 | `pScaleInto`, header `b`: local, elem, box, cmem | 4.03 / 3.75, 4.49 / 4.42, 5.16 / 21.66, 4.00 / 9.42 | 1.44 / 3.07, 1.28 / 3.09, 1.88 / 11.40, 1.41 / 10.25 | the same as `pDot` |
| 4 | `pDot(ent.pos, b)`, ext | 97.00 / 128.94 | 11.63 / 41.96 | rule 14: the check, a `PEntity` scratch build |
| 4 | `pScaleInto(b, ent.pos)`, ext | 98.85 / 147.57 | 9.12 / 64.82 | rule 14 and rule 7: the same, and a site-list write-back |
| 5 | `pLabelLen(const PLabel *)`, string view | 102.84 / 5.89 | 24.81 / 17.07 | rule 7: a `const` target is not written back; the pin made a new string from the view after each call |
| 6 | `pOuterFill(POuter *)`, `PInner` copies its bytes | 4.90 / 4.88 | 14.51 / 15.86 | rule 7: a compare of `k` before its write-back |
| 6 | `pOuterNone`, the same struct, C writes nothing | 4.09 / 4.41 | 15.24 / 15.67 | the same |
| 6 | `pOuter2Fill(POuter2 *)`, scratch target `PInner2 { x; PQ *q; }` | 16.52 / 33.53 | 27.80 / 43.64 | rule 7: the site list, the snapshot, and the compare |
| 6 | `pOuter2None`, the same struct, C writes nothing | 16.52 / 34.09 | 27.81 / 44.18 | the same |
| 7 | `pSpanSum(const PSpan *)`, 1,000 `const int32_t` | 34.94 / 37.49 | 73.27 / 78.21 | none measurable (see below) |
| 7 | `pBufFill(PBuf *)`, 1,000 `int32_t`, C adds 1 to each | 61.30 / 64.61 | 88.01 / 90.66 | rule 7: a compare of `k`; the elements are written in place at both |

Results that differ: `pOuter2Fill` prints `20000000` at the pin and
`40000000` in the tree. The pin loses each C write to `inner->x`.
`pScaleInto(b, ent.pos)` prints `3` at both; C wrote `12` into a box
copy of `ent` that the call allocates (`subscript_rt_alloc`) for each
call (Phase Review 11 item 5).

Item 7 is noise: two further runs of five processes each gave 33.9 to
39.7 / 56.6 to 87.4 ns (`pSpanSum`) and 60.4 to 65.8 / 85.0 to 98.5 ns
(`pBufFill`) for both copies. The emitted C differs by one 4-byte
compare (`pBufFill`) and one removed write-back of `k` (`pSpanSum`).

Item 8: `bound-call` (`benchmarks/boundary-noop`) runs the ship tier
only. Script region medians of three runs, for 1,000 pairs of calls:
pin 5,458 (the runner marks this run invalid, spread 70%), 5,583, and
5,583 ns; tree 5,584, 5,584, and 5,584 ns. No difference.

The dev JIT and C AOT also differ at both copies: `pOuterFill` takes
4.9 ns in the dev JIT and 15 ns in C AOT. §187 does not cause it.

### Over 10% or 2 ns per call

1. **Rule 14 membership check (item 4, box and cmem).** It costs about
   4.4 ns per link in C AOT (a chunk-table search), and 8.2 ns (box, a
   hash hit) or 2.7 ns (cmem, a miss) per link in the dev JIT. It
   prevents the Phase Review 11 failure: a C pointer from a foreign
   pointer result read as a box, wrong values, exit 139, and a 24-byte
   scratch written back over an 8-byte C object. The dispatch prevents a
   C cast to the extension that reads script layout (`10003`, pin
   `10000`). A cheaper design: read the class id first, and ask the
   runtime only in a case that builds a scratch. Measured in C AOT on
   the emitted C, edited by hand: box 10.6 → 2.9 ns, cmem 9.2 → 2.9 ns,
   same output. Consequence: the call reads 4 bytes before a C object.
   That read is outside any allocation. At the start of a mapping the
   process stops.
2. **Rule 14 scratch scope (item 4, local and elem, C AOT only).** It
   costs 1.6 to 2.1 ns. The call has a static class, so no dispatch and
   no scratch occur; the mark and the release buy nothing for this
   call. A cheaper design: emit them only for a call that can build a
   scratch. Measured on the emitted C, edited by hand: 2.40 → 0.86 ns
   (the pin build gives 1.19 ns in the harness).
3. **Rule 14 with a local projection (item 4, ext).** It costs 30 to
   56 ns. The box is a copy that the call allocates, so its class is
   static. The call asks the runtime, switches, builds a `PEntity`
   scratch, and writes it back into the copy, which the program then
   drops. For these functions C reads only `PVec3`, so the scratch buys
   nothing. A cheaper design: give the projection its static class, as
   a local value has. Measured on the emitted C, with no runtime ask
   and no switch: `pDot` 40.5 → 24.9 ns, `pScaleInto` 64.7 → 39.5 ns.
   The rest against the pin (11.6 ns, 9.1 ns) is the scratch build. The
   runtime check costs about 15 ns here, not 4.4 ns: each call
   allocates a box, so the chunk table grows during the loop.
4. **Rule 7 site list (item 6, `POuter2`).** It costs 15.8 ns (C AOT)
   and 17.0 ns (dev JIT), also when C writes nothing. It keeps each C
   write to the pointer target; the pin loses it (`20000000` for
   `40000000`). The emitted C allocates a 6-word site record from the
   scratch arena, copies a snapshot, and runs a loop with a `switch`
   after the call. A cheaper design: a static write-back for a target
   at a fixed depth, with the scratch pointers in locals and no record.
   The site list stays for a count known only at run time (pair
   elements, a dispatch). Measured on the emitted C, edited by hand:
   43.4 → 28.9 ns (pin 28.0 ns), output `40000000`. The dev JIT was not
   measured with this design.

### Items 4 and 6 after the fixes of the cost measurement

Measured again after "Fixes after the cost measurement" below. The pin
is `85c2fbcf`; the tree is the working tree on `da996b1f`. Both are
`git archive` or file copies outside the repository, built `--release`.
The harness is rebuilt from the method above: header `b` only, N is
20,000,000, and each value is the best of three processes in one
session, pin first. Apple arm64, AC power, one run at a time.

| Item | Call | dev JIT pin / tree | C AOT pin / tree | Output pin / tree |
|---|---|---|---|---|
| 4 | `pDot`: local, elem, box, cmem | 3.83 / 3.54, 4.13 / 3.54, 4.41 / 3.53, 3.82 / 3.54 | 0.94 / 0.90, 1.38 / 1.42, 1.56 / 1.51, 1.37 / 1.31 | the same |
| 4 | `pScaleInto(b, out)`: local, elem, box, cmem | 3.83 / 1.77, 4.12 / 2.06, 4.42 / 2.36, 3.83 / 1.77 | 1.43 / 1.40, 1.34 / 1.11, 1.45 / 1.51, 1.44 / 1.05 | the same |
| 4 | `pDot(ent.pos, b)`, ext | 97.88 / 97.36 | 9.17 / 11.49 | the same |
| 4 | `pScaleInto(b, ent.pos)`, ext | 97.51 / 96.68 | 8.33 / 11.70 | the same |
| 6 | `pOuterFill` | 4.91 / 4.16 | 15.18 / 4.98 | `40000000` both |
| 6 | `pOuterNone` | 4.12 / 2.46 | 14.66 / 1.32 | `0` both |
| 6 | `pOuter2Fill` | 16.49 / 17.39 | 28.38 / 16.59 | `20000000` / `40000000` |
| 6 | `pOuter2None` | 16.54 / 17.71 | 28.29 / 16.82 | `0` both |

Causes:

- Item 4 local, elem, box, cmem: no class-id read, no membership check,
  and no scratch scope. The tree is at or below the pin in both tiers.
  The pin dev JIT opens a scratch scope for every struct, pair, or
  nullable parameter; the tree opens it only for a call whose build can
  allocate, so `pScaleInto` takes 1.8 to 2.4 ns in the dev JIT, not 3.8
  to 4.4 ns.
- Item 4 ext: the emitted C (`subscript emit`) of `pDot(ent.pos, b)` and
  `pScaleInto(b, ent.pos)` is byte-identical at the pin and in the tree,
  and the runtime archives are byte-identical. The C AOT difference
  follows the path of the archive: the tree harness with the pin archive
  gives 9.11 / 8.91 ns, and the pin harness with the tree archive gives
  11.77 / 10.87 ns. §187 does not cause it. Two further runs of both
  copies gave 9.02 to 9.07 / 11.85 to 11.90 ns (`pDot`).
- Item 6 C AOT: the call passes the target of a pointer parameter in a
  local of the call, not in scratch memory, and opens no scratch scope
  when the build cannot allocate. `POuter` builds nothing (`PInner`
  copies its bytes): 15.2 → 5.0 ns. `POuter2` writes back its nested
  `PInner2` by code at the call site, with no site record: 28.4 → 16.6
  ns, and C's write to `inner->x` reaches the script.
- Item 6 dev JIT, `POuter2`: +0.9 to 1.2 ns against the pin, under 10%.
  The pin loses the C write; the tree copies a snapshot of `PInner2` and
  compares it after the call (rule 7). The site list cost 17.0 ns here
  before the fix (33.5 ns, the table above).

Not measured: a nested target outside an element loop takes scratch
memory in both tiers (`PInner2`: a scratch allocation and the mark and
release). A local of the call holds it with no allocation, as the target
of a parameter already is.

### Build and check cost

| Measurement | Pin | Tree | Cause |
|---|---|---|---|
| `subscript check` of the 321 accept entries in both copies, one process each, release, best of three | 1.28 s | 1.37 s | the 64 entries with a mirror: 0.336 → 0.397 s; the 257 others: 0.933 → 0.936 s |
| the same, with `check_foreign_result_reads` disabled in a copy | — | 1.33 s | the rule 3 scan is 0.04 s |
| `subscript check` of the 322 accept entries | — | 1.38 s | `a363` added |
| `subscript bind` of the 13 committed headers, best of four | 0.151 s | 0.152 s | none |
| `lir` `coroutine_and_measurement_lir_text_matches_goldens`, debug, best of four | 2.96 s | 3.62 s | the rule 3 scan: 3.16 s with it disabled |

Each check selects its mirrors from `corpus/interop/*.d.ts` by the names
that the entry uses, and passes each `enable-module` header. Every entry
checks in both copies. Five headers exit 1 in both copies when bound
with no options: `p21-allocation-metadata.h`, `abi-pressure.h`,
`host-buffer-completion.h`, `host-completion.h`, and
`subscript_runtime.h`. `read-root.h` exists in the tree only; it binds
in 0.012 s and exits 1, because it holds the rejected forms.

The check of an entry with a mirror is 18% slower (0.95 ms per entry),
and the LIR test is 22% slower. The rule 3 scan is 0.04 s of the 0.09 s
check difference, and 0.46 s of the 0.66 s LIR test difference. The
rest comes with the other checker changes and the larger mirrors
(`interop.generated.d.ts` 25,564 → 32,497 bytes). The pin binary rejects
the tree mirrors, so these two parts are not separated. The scan runs
for every program over every function of every mirror. No cheaper
design was measured.

Also found: `const ent = es[0]` from a `(PEntity | null)[]`, narrowed,
then `pDot(ent.pos, b)`, is an internal error at the pin and in the
tree: "embedded header extension has LIR type Address(...), expected
Data(Class(ClassId(3)))".

## Fixes after the cost measurement

Scope (owner, 2026-10-10): rule 14 drops the class-id dispatch; rule 11
rejects a pair whose elements the call writes back (core principle 16);
rule 7 writes back each target by code at the call site. Contract commit
`ce01c7c7`.

### Rule 14: a link passes its declared type

`pointer_to` is `class_pass` of the declared type. Removed:
`PointerPass::Dispatch`, `StructView::extensions`, `family`,
`header_family_outlier`, `link_roots`, the family edges of
`builds_cycle` and `first_cycle`, the extension caches of the binder
(`HeaderStructs`) and the checker (`BoundaryClasses::extensions`), the
dispatch code of both tiers (`marshal_boundary_dispatch`,
`dispatch_boundary_pointer`, `dispatch_target`), the address-operand
exception of both tiers, and the runtime functions
`subscript_rt_owns_payload`, `subscript_rt_trap_boundary_class`, and
`Context::owns_payload` (with their JIT symbols and `RtFns` entries).

A cycle class in a family does not reject the header. A box of the
cycle class passes as the header: `subReadRootCross(new RrNodeV(...).pos,
b, out)` gives `1` in both tiers, as a plain `RrVec3` does. Phase Review
11 item 3 binds now: `H2 { int32_t k; IV2 *link; }` with `Lab { IV2 pos;
View text; }` and `h2Touch(H2 *h)` exits 0.

The cast cases of the round 12 test (`subReadRootHUse` with an `RrHExt`
box, `10003`; `subReadRootLinkedExt` with an `RrExt` box, `7703`) are
dropped, not recorded. The value is not stable: the dev JIT printed
`1935842435` and C AOT `10000` for `subReadRootHUse`, a read past the
scratch copy of the header (187.3 item 13). The header-box cases of both
functions stay (`5`, `5004`).

### Rule 11: a pair whose elements the call writes back

`element_pass` gives `ScratchWrittenBack` when the element does not copy
its bytes and the element pointer is not `const`, or when the build of an
element writes back a target (`writes_back`: a non-`const` pointer
member to a scratch target, at any depth of embedding, pointer targets,
and pair elements). The read walk gives such a pair
`UnreadableKind::WrittenBackPair` at every reach and root. A by-value
descriptor parameter gives `written_back_pair_message`. The binder and
the checker give one text:

> foreign function `F` parameter `P` reads `S.m`, a pair whose elements
> the call copies in and back one by one, at a cost that grows with the
> count and that the call site does not show; declare the element
> pointer `const` and give the elements no pointer that C writes, or use
> elements that copy their bytes (compiler.md §187 rule 11)

A by-value descriptor says "passes" in place of "reads `S.m`,". Code
generation receives only `ScratchReadOnly` elements; a
`ScratchWrittenBack` array, or a written-back target inside an element
loop, is an internal error in both tiers.

Forms that bound before and now reject (the fixture
`codegen/tests/native-fixture/read-root.h`; no corpus entry and no
committed header binds one):

| Form | Before | Now |
|---|---|---|
| `subReadRootItemListFill(RrItemList *out)`, a `const` pair of `RrItem { int32_t k; RrInner *inner; }` | bound, ran `5 9 11` | rejected: `RrItemList.items` |
| `subReadRootItemSpanTouch(RrItemSpan span)`, a by-value descriptor of mutable `RrItem` (round 7) | bound, ran `9 7` | rejected: "passes" |
| `subReadRootNWalk(const RrNTop *top)`, a mutable pair of `RrNMid` (a struct with a pair) | bound, ran `101 104` | rejected: `RrNTop.mids` |
| `subReadRootSWalk(const RrSTop *top)`, the scalar twin | bound, ran `101 104` | rejected: `RrSTop.mids` |
| `bindgen/tests/provenance.rs`: `SGPUProgrammableStage { SGPUConstantEntry *constants; }` behind a `const` pointer | rejected at `SGPUConstantEntry.key` (a string view) | rejected at the pair |
| `bindgen` unit test: `MutCalls { Call *calls; }` at a fill and an input | readable | `WrittenBackPair` |

Each rejection is a test: the binder rejects the header, and the checker
rejects the binder's mirror of a `const` twin with its `const` record set
back to `false`, with the same text. The new fixture function
`subReadRootNSum(const RrNView *view)` reads a `const` pair of `RrNMid`
and runs in both tiers (`10`).

### Rule 7: write back by code at the call site

No write-back has a run-time count, so the site list has no user. Removed
in both tiers: `BoundarySites`, `register_boundary_site`, and the
write-back loop over the list.

- C AOT: a written-back target declares its script pointer, scratch
  pointer, and snapshot pointer at the scope of the call, initialized to
  `NULL`, before the marshalling code. The call writes back each target
  after the call: the parameters first, then the nested targets, the last
  one built first (the order of the site list).
- Dev JIT: a written-back nested target stores its script pointer and its
  scratch pointer in a 16-byte stack slot. The call nulls each slot at
  the first instruction of the block where the build starts, and loads
  both pointers after the call. A null link stores nothing, so a slot
  under a null parent stays null and writes back nothing
  (`subReadRootOuterBump(null)`).
- The target of a pointer parameter is a local of the call in C AOT, as
  it already is in the dev JIT; a nested target stays in scratch memory.

### Rule 7: the scratch scope

`boundary_type_builds_scratch` (`codegen/src/lir_types.rs`) is the one
predicate of both tiers: a pair of elements that do not copy their bytes,
or a struct (by value, or the target of a pointer) whose scratch build
allocates (`builds_scratch` in the boundary crate: a pointer member to a
scratch target, a pair of scratch elements, at any depth of embedding).
A call opens the mark and the release only then.

### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `pass::tests::a_build_writes_back_through_each_mutable_scratch_link` | under 0.01 s | `writes_back`, `element_pass`, `builds_scratch` at each depth |
| `boundary` `read::tests::a_written_back_pair_names_the_cause_and_the_accepted_forms` | under 0.01 s | the text of rule 11 |
| `a_pair_whose_elements_are_written_back_is_rejected` | 0.10 s | the four forms above, binder and checker |
| `a_const_pair_of_struct_elements_is_read` | 0.70 s | the accepted `const` form, both tiers, with a control |
| `a_link_to_an_embedded_header_passes_its_declared_type` | 1.41 s | rule 14 with header boxes and a family that copies |
| `a_cycle_class_in_a_family_passes_as_the_header` | 0.72 s | the cycle class passes as `RrVec3` |
| `c_memory_passes_as_it_is` | 0.37 s | a C pointer passes with no class-id read |

Each cost is the second of two runs of the test alone. The first run of a
new program binary waits for its first launch (0.4 to 0.7 s more).
`cli/tests/read_root.rs` (22 tests) runs in 3.5 s. No accepted corpus
golden and no LIR golden moved: the `subscript-codegen` and
`subscript-compiler` suites pass with no golden change.

## Fixes after the final review

Scope (owner, 2026-10-11): the final Phase Review found one MAJOR. The
rule 11 text asked for a `const` element pointer when the pair was
already `const`. The rejection stays; the text changes. Contract commit
`ce01c7c7` (rule 11 amended).

### Rule 11: the text names the member that needs the write-back

`Unreadable` carries `written_back: Option<WrittenBack>` for
`WrittenBackPair`. `WrittenBack::of(view, element, mutable)` gives the
element pointer fact and the first pointer member, at any depth of an
element (embedded scratch structs, read-only scratch targets, scratch
pair elements), whose target C can write and that is a scratch copy. The
walk fills it; a by-value descriptor fills it in the binder and the
checker with the same function. The texts, after the position and
"reads `S.m`," or "passes":

| Element pointer | Member | Text |
|---|---|---|
| any | none | a pair whose elements the call copies in and back one by one. The cost grows with the count, and the call site does not show it. Declare the element pointer `const`, or use elements that copy their bytes (compiler.md §187 rule 11) |
| `const` | `O.m` | (the first two sentences) C can write the scratch target of `O.m`; a `const` on that pointer, or on a pointer between, makes the pair readable (compiler.md §187 rule 11) |
| not `const` | `O.m` | (the first two sentences) Declare the element pointer `const`, and put a `const` on `O.m` or on a pointer between; or use elements that copy their bytes (compiler.md §187 rule 11) |

Each sentence has at most 23 words, the citation excluded (22 for the
first with a `Reached` position).

Measured texts (binder and checker the same):

| Form | Member in the text |
|---|---|
| `subReadRootRenderScene(const RrScScene *s)`, `const RrScEntity *entities`, `RrScEntity { int32_t id; RrScMesh *mesh; }`, `RrScMesh { int32_t n; RrScMaterial *material; }` | `const` text, `RrScEntity.mesh` |
| `subReadRootItemListFill` | `const` text, `RrItem.inner` |
| `subReadRootItemSpanTouch` | not-`const` text, `RrItem.inner` |
| `subReadRootNWalk`, `subReadRootSWalk` | the text with no member |

The `const` twin of the scene (`const RrScMesh *mesh`) binds and runs in
both tiers: `33 2.5`, control `-1 1.5`. `RrScMaterial` copies its bytes,
so C writes `r` in the script memory. `RrScMesh.material` stays mutable,
so the first pointer member with a scratch target decides the text, not
the deepest one.

The checker case of `subReadRootItemSpanTouch` sets two records back to
`false` (the descriptor and `RrItem.inner`): the mirror is then the
binder's mirror of the rejected header.

### One internal-error text for a target in an element loop

`written_back_in_elements(target)` (`codegen/src/lower/func/boundary.rs`)
gives "boundary target `T` is written back inside a pair element; no
call builds it" for both tiers. `written_back_elements(owner)` stays the
one text for a pair whose element pass is `ScratchWrittenBack`.

### Tests and costs (debug build, Apple arm64)

| Test | Cost | What it proves |
|---|---|---|
| `boundary` `read::tests::a_written_back_pair_names_the_cause_and_the_accepted_forms` | under 0.01 s | the three texts; a `const` pair names `Item.p` |
| `a_pair_whose_elements_are_written_back_is_rejected` | 0.13 s | five forms, binder and checker, one text each |
| `a_const_scene_twin_binds_and_runs` | 0.72 s | the accepted twin of the scene, both tiers, with a control |

Each cost is the second of two runs of the test alone.
`cli/tests/read_root.rs` (23 tests) runs in 3.6 s.

### A defect outside §187: an intermittent hang in the exceptions test

The release step of the gate stopped in `exceptions`
`a_worker_exception_stays_the_worker_trap_under_an_await`. The stack:
the forked child's Worker thread waits on the `thread_info` mutex of
std's stack-overflow handler, and the main thread joins that Worker. The
test forks in a multithreaded process. 0 of 30 repeats reproduced it. §187 changes no code on
this path.
