<!-- §12 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 12. P5 C-header binding vertical slice

The language's founding purpose (plan §4): express zero-copy C-ABI
interop. P5 proves it against a **neutral synthetic C header**
that exercises all five interop patterns — no real-world library is
named or depended on (invariant 4; CLAUDE.md repo hygiene).

### 12.1 The synthetic header

A committed C header (`corpus/interop/<name>.h` or similar) authored for
this slice, containing only the constructs the five patterns need and
**no unions, no bitfields** (the layout-identity guarantee is about C
structs/enums/pointers/function-pointers/opaque-handles only). It
exercises, one construct per plan-§4 pattern:

1. an intrusive extension chain (a common embedded header with a `next`
   pointer and a type tag, plus ≥2 chainable extension structs);
2. a `(pointer, count)` array-pair API;
3. a length-carrying string-view struct (`{ const char*; size_t; }`,
   not NUL-terminated);
4. a callback API (function pointer + `void* userdata`);
5. an opaque handle with create/retain/release.

### 12.2 Mirror generator

A generator (`bindgen`-style, this project's own) reads the header and
emits the ambient `.d.ts` mirror per the Q13 boundary typing rules
(`specs/blocks/collisions.md` §2 Q13), already decided and binding:
opaque handles → branded empty interfaces; struct pointers and zeroable
by-value sub-layouts → `X | null`; string views → `string`; flag sets →
`u64` aliases; callback userdata → `object | null` narrowed with `as`.
The generated mirror is **never hand-edited** (CLAUDE.md core principle
6); regenerating from the pinned header reproduces it byte-for-byte
(a test).

### 12.3 `offsetof` assertion suite — the layout proof

Invariant 1 (C-ABI-identical layout) is machine-verified here, not
asserted. For every struct the mirror exposes, a generated test asserts
that the language's lowered layout matches the C compiler's:
`offsetof`/`sizeof`/`_Alignof` of each field and the whole struct, taken
from the real C header via the platform C compiler, equal the language
compiler's computed offsets/size/alignment. A mismatch fails the suite.
This runs for the dev targets (host) and is the concrete discharge of
"machine-verifiable via `offsetof` assertions" (plan §3 invariant 1).

### 12.3a Dev-tier boundary-struct marshaling: AAPCS64 and Win64

The ship tier is C emission (§11), where the platform C
compiler performs all boundary-struct argument marshaling and is correct
by construction on every ship target (arm64 iOS/Android and
`x86_64-unknown-linux-gnu`). The dev JIT must hand-build the C-ABI call, and passing
a boundary **struct by value** across a foreign call is ABI-specific. The
marshaler branches on the **target ABI**, not merely the architecture,
because x86-64 hosts split by OS: `x86_64-pc-windows-msvc` is Win64,
`x86_64-unknown-*` is SysV, and the two disagree on struct passing.

Implemented and verified:

- **AAPCS64 (arm64)**: an HFA/HVA is passed component-wise in float
  registers; any other composite of at most 16 bytes is passed in
  consecutive general registers as **eightbyte images** — the struct's
  bytes at their C offsets, one register per eightbyte — and a larger one
  is passed by reference to a caller copy (AAPCS64 B.4). *(Corrected
  2026-08-03 by §47, OBS-4: this rule read "its components as arguments",
  which is a different and wrong marshaling — it silently delivered wrong
  values for any by-value struct with two or more sub-eightbyte integer
  fields, since the callee reads a whole eightbyte where the caller had
  written one field. `{i64,i64}` and HFAs were unaffected, which is why
  it survived.)*
- **Win64 (`x86_64-pc-windows-msvc`)**: a struct whose total size is
  exactly 1, 2, 4, or 8 bytes is passed **by value in one integer
  register** — the whole struct as a single packed integer of that width,
  with no HFA/float-register special case and no multi-register packing;
  every other size is passed **by reference** to a caller copy. (A callback
  field expands to trampoline+binding = 16 bytes, so any struct carrying
  one is by-reference on Win64.)

Implemented 2026-08-09 and verified byte-exact on the x86-64-linux gate
(interop 13/13, golden 27/27, dev-JIT ≡ ship-C-AOT ≡ golden;
`specs/tracking/linux-portability.md`):

- **SysV (`x86_64-unknown-*`, System V AMD64)**: the struct is split into
  eightbytes and each eightbyte gets a class. An eightbyte whose bytes are
  all float (`f32`/`f64`) is class **SSE** and passes in the next SSE
  register as an `F64`/`F32` image; every other eightbyte is class
  **INTEGER** and passes in the next general register as an `I64` image,
  with sub-eightbyte fields packed at their C offsets. This is the AAPCS64
  eightbyte-image rule (§47) plus the per-eightbyte INTEGER/SSE split that
  AAPCS64 does not make. A struct of at most 16 bytes uses one or two
  eightbytes. A larger struct, or one with an unaligned field, is class
  **MEMORY**: an argument passes **on the stack by value** — a copy, not a
  pointer — and a return passes through a hidden pointer. SysV and AAPCS64
  diverge here: AAPCS64 passes a larger struct **by reference**, so the
  by-reference `Indirect` path stays AAPCS64/Win64 only.

*(2026-09-05, measured on the x86-64 Linux gate host.)* **A MEMORY-class
argument occupies whole eightbytes.** The psABI stack is eight-byte
aligned and each stack argument takes an integral number of eightbytes,
so the dev JIT rounds the caller copy up to a multiple of 8, zero-fills
it, and then copies the struct's bytes into it. The Cranelift
`StructArgument` size is the rounded size. Cranelift's x64 backend
asserts that the size is a multiple of 8, so an unrounded size is a
panic in the JIT thread, not a mis-marshal. `plan_aggregate_arg` carries
the rounded stack size, because the emission is its consumer (core
principle 8).

The shape that shows it is a struct whose width is over 16 bytes and is
not a multiple of 8. `EngineTransform` (`examples/engine/engine.h`,
`{ bool; float; float; float; uint16_t; }`) measures size 20 and align 4
on this host, and `e09-c-structs-and-slices` and
`gate/two-header-binding` pass it by value. The two corpus MEMORY shapes
this section names — `SubCallbackInfo` and a126's `{ i64, i64, i64 }` —
are both 24 bytes, so neither reaches a rounded size. Red measured at
`90aa5cb` in both profiles: each entry panics with "StructArgument size
is not properly aligned". The 2026-08-09 SysV verification above
predates the `ec1d8be` rewrite, so it does not cover this path.

On any host whose ABI is none of AAPCS64, Win64, or SysV, lowering a
foreign call that passes or returns a boundary struct by value stays a
**loud codegen error**, never a silent mis-marshal, since dev-JIT ≡ ship-C
equivalence is otherwise unverifiable there.

*(2026-08-28.)* This section read "Implemented and verified" for a month
while Win64 and SysV were absent. `5807d7b` (§68 step 2) replaced the HIR
consumer with the LIR transcriber and carried AAPCS64 across alone, so
every x86-64 dev host met the loud error above. Only the arm64 reference
machine ran, so no gate reported it
(`specs/tracking/windows-portability.md`). The guard that should have
reported it, `boundary_struct_by_value_supported`, was a `#[cfg(test)]`
predicate lowering never called — core principle 9's shape. The three
ABIs are one function, `plan_aggregate_arg(abi, leaves, size)`; lowering
and the test both read `AggregateAbi::of(triple)`, and the test pins the
ABI identity per triple, not a bool.

The corpus exercises the SysV **MEMORY argument** path directly — a 24-byte
`SubCallbackInfo` (`{ fn-ptr, void*, void* }`, in `a25`–`a90`) and a
`{ i64, i64, i64 }` triple (`a126`) are each passed by value — so that path
is **implemented**, not staged: the dev JIT builds the struct in a caller
slot and passes it by value on the stack (Cranelift `StructArgument`),
never the by-reference `Indirect` path. What stays a loud error on SysV is
a struct **return in SSE-class registers** (a float or HFA return in
XMM0/XMM1): the dev JIT models no float return register on **any** ABI —
the AAPCS64 and Win64 HFA-return cases are the same loud error — so this is
a shared, accepted follow-up, not a Linux-specific gap. An INTEGER-class
SysV return (RAX, then RDX) and a MEMORY return (hidden pointer, like the
other ABIs) are implemented.

Two SysV argument shapes stay a **loud error**, each a silent-mis-marshal
risk the differential gate cannot see (no corpus entry exercises them):

- **Argument register pressure.** psABI §3.2.3 step 5: when an aggregate's
  eightbytes do not all fit in the remaining argument registers, the whole
  aggregate reverts to the stack. The dev JIT pushes eightbytes as
  independent scalars, so it cannot split-then-revert; when it detects that
  the remaining GP/SSE registers cannot hold every eightbyte it raises a
  loud error rather than mis-marshal. The stack-revert path is a tracked
  follow-up. AAPCS64 has the same unmodeled revert and is the same
  follow-up.
- **An `f16` leaf in a register-class eightbyte.** The psABI classifies
  `_Float16` as SSE, but `f16` is storage-only here (§16.2) and lowers to
  `I16`, so the eightbyte would classify INTEGER. A register-class SysV
  aggregate with an `f16` field is a loud error; a MEMORY-class one (a byte
  copy) is unaffected. Full SSE classification of `f16` across both tiers
  is a tracked follow-up.

A length-carrying **string view** (`{ const char*; size_t; }`) and a
**`(pointer, count)` array descriptor** (`{ const T*; size_t; }`) are each
a 16-byte C aggregate passed **by value**, so they take the same
target-specific path as any boundary struct — corrected from the earlier
claim that `(ptr,len)` pairs are target-neutral, which held only because
AAPCS64 and SysV both happen to pass a 16-byte two-pointer aggregate in two
registers (the same two argument slots the pair would occupy). Win64
disproves it: a 16-byte aggregate is passed **by reference**, so the dev
JIT must build the descriptor in a caller slot and pass its address, not
two registers. Only genuinely scalar/pointer boundary args — a single
handle, `object|null`, a lone pointer — are target-neutral. Each ABI is
validated by the standing differential gate (dev-JIT ≡ ship-C-AOT ≡ golden)
on a host of that ABI: the AAPCS64 path on arm64, the Win64 path on
Windows-x64, and the SysV path on x86-64 Linux
(`specs/tracking/linux-portability.md`).

### 12.4 Headless end-to-end slice on both tiers

Corpus accept entries (a25+, numbered here) written in the language
against the generated mirror, one per pattern plus one that composes all
five, exercised headless (no GPU, no window, no external device —
CLAUDE.md core principle 4). A minimal C implementation of the synthetic
header (committed, compiled and linked into the test) provides the
callee side. Each entry runs under **both tiers** and its output is a
committed golden; the standing differential gate (§11) extends to them:
dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact. Q16 (how a corpus program
obtains a host-created handle) is decided here: the host harness creates
the handle and calls an exported entry, or the entry creates it through
the synthetic `create` — state which per entry.

### 12.5 Gate (§4)

The five patterns each have a passing headless corpus entry on both
tiers with a committed golden; the mirror regenerates byte-identically
from the pinned header; the `offsetof` layout suite is green on the dev
target. Zero real-world-library references (reference sweep clean).
