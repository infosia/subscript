<!-- §23 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 23. P25 — no header is privileged

Design invariant 4 says the host presents C headers and the language
binds those, and that **no specific host header is privileged by the
language**. The binding path does not implement that today: it binds
exactly one header, `corpus/interop/interop.h`, and a second header
cannot reach either tier.

Found 2026-07-28 while contracting `examples.md`, whose C-integration
examples bind a host facade of their own. The corpus never noticed
because the corpus has only ever had one header to bind.

### 23.1 What is wrong, with evidence

1. **The ship tier includes the fixture by name.** `codegen/src/cemit.rs`
   emits `#include "interop.h"` whenever the module has any foreign
   function, and declares the callback trampoline with that header's
   `SubStringView`.
2. **The ship tier names the fixture's descriptor structs.**
   `interop_array_pair_desc` maps an element type to a fixed C aggregate
   name — `SubBufferView` for `u32`, `SubSlice<T>` otherwise. Those
   names belong to the fixture, not to the language.
3. **The mirror throws away the name the emission needs.** `bindgen`
   absorbs a `(pointer, count)` descriptor into `T[]` at use sites and
   emits no record of the C struct it came from, so the emission has
   nothing to recover and a table in the compiler is the only thing
   left.
4. **The dev tier resolves foreign symbols from a fixed list.**
   `codegen/src/jit.rs` holds an `extern "C"` block naming the fixture's
   28 symbols and registers them by address. `cranelift-jit` 0.125.4
   falls back to `dlsym(RTLD_DEFAULT)` on Unix and to `GetProcAddress`
   over loaded modules on Windows, so an unregistered symbol resolves by
   accident on Unix and is not guaranteed to resolve on Windows —
   a portability trap, not a substitute for registration.
5. **The link line is the fixture's.** `codegen/src/aot.rs` passes
   `-I corpus/interop` and `corpus/interop/interop.c` unconditionally,
   and `codegen/build.rs` compiles that file into the crate.

The consequence is one sentence: **a host cannot bind its own header.**
That is the project's headline interop claim, and it is currently true
only of the fixture.

### 23.2 The rule

> Every C name the emitted code needs is **recovered from the bound
> mirror**. The compiler, the two tiers, and the runtime contain no
> identifier from any particular header.

The fixture keeps working by becoming **an argument** — one native
library among any number the caller supplies — rather than a case in the
binding path. The test of the rule is deletion: removing the fixture
must require touching test scaffolding only, never `compiler/src` or
`codegen/src`.

### 23.3 Mirror provenance

`bindgen` records in the mirror what the ship tier's C emission needs and
cannot otherwise know:

- the **include spelling** for the header the mirror was generated from,
  as the host would write it (`engine.h`), never a filesystem path;
- for every absorbed `(pointer, count)` descriptor, the **C aggregate
  name**, its element C type, its mutability (the const borrow and the
  out/mutable array are different types — §14.3), and the parameter it
  belongs to;
- the **C typedef name** of every callback type, so the trampoline can be
  cast to the type the header declares at the point of use.

Constraints on the spelling, not the spelling itself: it must keep the
mirror `tsc`-clean (invariant 5), it must be generated and covered by the
byte-identical regeneration test (§12.2), and a mirror the compiler
cannot parse provenance from is a **loud error at ingestion**, never a
silent fallback to a built-in table. Recording it per parameter rather
than per element type is required: one header may declare both a const
borrow and a mutable out-array of the same element type, and the
element type alone does not distinguish them.

### 23.3a One callback shape is bindable, and a mirror must refuse the rest

The runtime has **exactly one** C-ABI callback trampoline, and its
signature is fixed:

- `runtime/src/ffi.rs` — `subscript_rt_cb_trampoline(message: SubStrView,
  userdata1: *mut u8, userdata2: *mut u8)`, three C parameters;
- `codegen/src/lower/mod.rs` declares it to the dev tier as
  `[I64, I64, I64, I64]` — the view flattened to two registers plus the
  two userdata;
- `codegen/src/cemit.rs` stores it into a header's callback field through
  a cast to that field's declared typedef.

So the **only** bindable callback is `(string view, userdata, userdata)
→ void`, and the language function value behind it receives
`(message, userdata1, userdata2)`.

**Nothing in the toolchain checks this.** A mirror may declare a callback
of any shape, the checker accepts a matching lambda, and the lowering
installs the three-parameter trampoline behind it. The C API then calls
it with its own signature: an extra leading parameter lands where the
view's data pointer is read, and the view's length lands where the
binding pointer is read and dereferenced.

*(Found 2026-07-28. The examples facade's first draft declared
`(EngineEventKind, EngineStringView, void*, void*)`; `bindgen` emitted the
mirror, and no stage of either tier objected.)*

**`bindgen` must reject a callback typedef whose signature the trampoline
cannot serve**, naming the typedef and the supported shape — the §23.8
kill criterion applied to callbacks: a mirror the tiers would mis-marshal
must not be written. A header that needs to deliver more than the view
and the two userdata slots delivers it through a separate call the script
makes, not through an extra callback parameter.

**The check is on reachability, not on presence.** It applies to a
function-pointer typedef that can cross the boundary — used as a mirrored
struct's field, or as a foreign function's parameter or return. A typedef
a host declares and the boundary never touches cannot be mis-marshaled,
and rejecting the header for it would make headers unbindable for a
reason the language does not have. Such a typedef is simply not mirrored.

Rejecting on presence was the first implementation, and it fails P25's own
purpose: one unrelated function pointer anywhere in a production header —
an allocator hook, a debug sink — would refuse the whole header.

Generalizing the trampoline to arbitrary callback signatures is **not in
P25's scope**. This section makes the existing limit visible and loud; a
host that needs more is a later phase with its own contract.

### 23.3b Carried forward — the inherited-precedent audit

`specs/tracking/inherited-precedent-audit.md` pre-registers a sweep for
one defect class this phase produced: a requirement carried from an older
artifact by analogy, without re-deriving whether the destination needs it.
One §23.3 provenance record is already named there as suspect. The sweep
runs at or after this phase's Phase Review; its scope and pre-registered
outcome are in that file, not restated here.

### 23.4 Ship tier

The emitted C includes **each ingested mirror's header**, in ingestion
order, and no other. Descriptor struct names and callback typedef names
come from provenance.

The generic callback trampoline is declared with a **locally emitted,
layout-identical view struct** under a reserved `sub_` name rather than
with a header's type; the runtime already defines that layout as
`SubStrView` in `runtime/src/ffi.rs`. At the point where the trampoline
is stored into a header's callback field it is cast to that field's
declared C typedef, recovered per §23.3. Layout identity (invariant 1)
is what makes the cast sound; it is not a convenience.

### 23.5 Dev tier

Foreign symbol resolution becomes explicit and caller-supplied. No
`extern "C"` block naming a particular header's symbols remains in
`codegen/src`. Registration is by address, as today; the fixture's table
moves to the corpus gate's own support code.

**The dlsym fallback is not relied on.** A foreign symbol a caller did
not register is a run error naming the symbol, not a lookup that happens
to succeed on one platform. This is the Windows half of the portability
record (`specs/tracking/windows-portability.md`).

### 23.6 The surface a caller uses

One value type describes a native library the caller links: its include
directories, its C sources for the ship tier's compile, and its symbol
table for the dev tier's JIT. Both runners take a set of them. Taking
function addresses across a language boundary is `unsafe`; the
constructor carries the SAFETY contract (the addresses outlive every run
and match the C signatures the mirror declares).

`emit-c` (`codegen/src/bin/emit-c.rs`) additionally accepts explicit
source and mirror paths instead of only a corpus entry id, and a flag to
suppress the generated `entry.c` — a host that owns `main` supplies its
own. The corpus-entry form stays, because `device-link.sh` uses it.

### 23.7 Corpus and gate (pre-registered)

**A second fixture, and the two bound together.** The examples' host
facade (`examples/engine/engine.h`, `examples.md` §4) is the second
header. Two new gate programs:

1. one binding **only** the engine facade — the first time any header
   other than the fixture is bound on either tier;
2. one binding **both** headers in a single program — the proof that
   binding is per-mirror and that no include, descriptor name, or symbol
   table is global.

Both run under dev-JIT and ship-C-AOT and are compared byte-exact, as
every corpus entry is. They live in the examples gate crate
(`examples.md` §7.6) with committed goldens, not in `corpus/accept/`:
that crate is what links `engine.c`, and the corpus stays free of a
dependency on `examples/`.

**No existing golden moves.** This phase changes which C names the
emission writes down, not what any program computes. An `.expected` that
moves is a finding.

#### 23.7a Corpus first — an extension payload read back through the chain

The intrusive extension chain is one of the five §4 patterns, and the
project exercises only half of it: `subDeviceCreate` **counts** nodes
(`interop.c`), no accept entry mentions `SubChainExtA`–`SubChainExtD`
(`grep ChainExt corpus/accept/` is empty), and no fixture function reads
an extension's payload. Reading the payload is the point of the pattern —
it is why a chain node carries a tag — and nothing pins it.

*(Found 2026-07-28 by the fresh-context review of the examples facade,
whose option walker reads payloads. Recorded here rather than in the
examples work: a semantic no corpus entry covers is a corpus gap, and
`examples.md` §1 forbids an example from being the first place it appears.)*

Two consequences, both before the facade's walker is accepted:

1. **The fixture grows one payload-reading walk** — a function that walks
   a chain, switches on `sType`, and folds each extension's payload
   scalars into an observable. Structs only, as §12.1 requires; the
   `offsetof` suite already mirrors `SubChainExtA`/`SubChainExtB`.
2. **An accept entry pins it**: a program that builds an extension in
   script, passes that extension's **embedded header field** into the
   `Struct | null` chain slot, and observes the payload the callee read
   back. This is the semantic the facade depends on — that the address
   crossing the boundary is the live struct's own storage, not a copy of
   the header field — and it is the one a node count cannot discriminate,
   because a copy of a header has the same `next` and yields the same
   depth.

**The precondition the pattern carries is documented, not engineered
away.** A callee that switches on a tag and casts to the matching
extension assumes the node is that extension's embedded header; a
production chain API assumes exactly this. A facade may therefore keep
payload-bearing options, provided the header states the precondition at
the declaration — and provided the entry above exists, so the spelling the
facade needs is the spelling the corpus teaches.

### 23.8 Exit criteria (kill or pass, pre-registered)

1. **A header other than the fixture binds and runs**, byte-identical on
   both tiers, against a committed golden (§23.7 program 1).
2. **Two headers bind in one program** (§23.7 program 2), byte-identical
   on both tiers.
3. **No fixture identifier or path survives in the binding path.** A
   search over `compiler/src` and `codegen/src` returns nothing for the
   fixture's type and symbol names (`SubSlice`, `SubBufferView`,
   `SubStringView`, `SubLogCallback`, `SubWaitList`, the `sub…` foreign
   symbols) **and** for the fixture itself — `interop.h`, `interop.c`,
   the `corpus/interop` path, and any identifier naming it such as
   `interop_dir` or `register_interop`. Matches under `corpus/`,
   `examples/`, `tests/` and `build.rs` are expected and are not
   violations.

   *(The name list was type names only until 2026-07-28. Stage 3 satisfied
   it by rewriting a doc comment while `aot.rs` still resolved
   `corpus/interop` and compiled `interop.c` into every link — Stage 4's
   scope, so not a violation there, but a criterion satisfiable by
   renaming is not a criterion. Criterion 4 is what actually establishes
   the property; this one is its cheap precheck and must not be weaker.)*
4. **Deleting the fixture touches test scaffolding only** — demonstrated,
   not asserted: the deletion compiles with no edit under
   `compiler/src` or `codegen/src`. The deletion is not committed.
5. **A missing registration fails loudly.** A program calling a foreign
   function whose symbol was not supplied reports an error naming the
   symbol, on both tiers and on both a Unix and a Windows host — not a
   dlsym hit.
6. **A mirror without parseable provenance is rejected at ingestion**,
   with the offending mirror named.
7. **`bindgen` regeneration stays byte-identical** (§12.2) with the
   provenance records present, and the mirror stays `tsc`-clean.
8. **A callback shape the trampoline cannot serve is rejected** (§23.3a),
   naming the typedef and the supported shape, with a test per rejected
   shape — an extra parameter, a missing userdata slot, a non-`void`
   return — and a test that an **unreachable** function-pointer typedef of
   an unsupported shape does *not* refuse the header.
9. Standing differential gate green; `tsc` clean; clippy at its baseline.

**Kill criterion.** If a descriptor's C name cannot be recovered for some
header shape the generator otherwise accepts, `bindgen` **fails on that
header** naming the construct, as it already does for an unmapped type
(§13.1). A mirror the ship tier would mis-marshal must not be written.
