# P5 — C-header binding vertical slice: evidence

Status: in progress, 2026-07-23. Contract: `specs/blocks/compiler.md`
§12; plan §4.

## Readiness (pre-implementation survey)

Solid foundations: C-ABI layout engine (`codegen/src/layout.rs`,
per-field offsets, C-correct, unit-tested); value-class-as-C-struct in
both tiers; `object` / `object | null` type with `as` narrowing (C7
boundary form, enforced boundary-only); 64-bit bitwise (Q18); the
`Linkage::Import` mechanism (how `sub_rt_*` are declared).

Greenfield (nothing exists): `corpus/interop/*.h`; a bindgen/mirror
generator; `.d.ts` ingestion (parser is `dts:false`; ambient surface
hardcoded in `compiler/src/ambient.rs`; `interface`/`type`/`declare
function` all rejected S100 by `collect_decl`); a foreign-function
concept (`hir::Callee`, `FnKey`, both backends only know runtime +
script calls); the `Struct | null` boundary null form (`resolve_union`
rejects value-class-with-null); a chain-slot address-of; the callback
trampoline (language `SubFn{code,env}` ≠ C `(fnptr, void* userdata)`);
`layout.rs` offsets are `pub(crate)`, not exposed for an external
`offsetof` test.

## Staging (dependency order)

- **P5.1 — synthetic header + `offsetof` layout proof (§12.1, §12.3).**
  Independent, discharges invariant 1 machine-verifiably (the founding
  layout claim), low risk. Author the neutral synthetic header (five
  patterns' structs, no unions/bitfields); expose per-field layout by
  name; a generated test asserts language layout == platform C compiler
  `offsetof`/`sizeof`/`_Alignof` for every struct. Value-class
  equivalents may be hand-authored here; retargeted to the generated
  mirror at P5.2.
- **P5.2 — mirror generator + ingestion + foreign-function machinery
  (§12.2).** The bulk. Bindgen emits the ambient `.d.ts` per Q13;
  byte-identical regeneration test; checker ingests it (`interface`→
  branded handle, `type`→`u64` alias / struct-ptr, `declare function`→
  foreign symbol); a foreign-call path across HIR/`FnKey`/both backends;
  the `Struct | null` boundary form; the callback trampoline; the
  chain-slot address-of.
- **P5.3 — both-tier corpus slice + gate (§12.4, §12.5).** Corpus a25+
  (one per pattern + one composing all five) against the generated
  mirror; a committed minimal C implementation of the header as the
  callee; goldens; the standing gate (§11) extends to them, byte-exact
  on both tiers; Q16 (handle acquisition) decided per entry.

Each stage: contract already in §12 → handoff → independent
verification → no-context Phase Review → fix → tracking. The standing
gate and the reference sweep (no real-world-library names) hold
throughout.

## P5.1 — synthetic header + offsetof layout proof: COMPLETE (2026-07-23)

`corpus/interop/interop.h`: neutral synthetic fixture (all `Sub`-prefixed,
no external project named), structs/enums/opaque-handle/fn-ptr typedefs
only, no unions/bitfields, all five plan-§4 patterns plus two
padding-exercising payload structs.

`codegen/src/layout.rs` gains `pub fn value_class_layouts(&Module) ->
Result<Vec<StructLayout>>` (`#[must_use]`, name↔offset join over the
positional `field_offsets`). `codegen/tests/offsetof_layout.rs` is the
proof: it generates a C probe that `#include`s the header, compiles it
with the platform `cc`, runs it, and asserts the language layout equals
the C compiler's `sizeof`/`_Alignof`/`offsetof` for every mirrored
struct. A missing `cc` fails (not skips).

**Invariant 1 machine-verified — language layout == C for all 8 mirrored
structs, zero disagreement:**

| struct | size/align | notable |
|---|---|---|
| SubChainHeader | 16/8 | sType 0, next 8 |
| SubChainExtA | 24/8 | header 0, intensity 16, flags 20 |
| SubChainExtB | 32/8 | header 0, scale 16, level 24 |
| SubBufferView | 16/8 | items 0, count 8 |
| SubStringView | 16/8 | data 0, len 8 |
| SubCallbackInfo | 24/8 | callback 0, userdata 8, userparam 16 |
| SubTransform | 88/8 | basis[16] 0, bone 64, weight 72 (interior gap), visible 80 |
| SubSample | 24/8 | a(bool) 0, b(f64) 8 (7-byte gap), c 16, d 20 |

Pointer/`size_t`/handle/fn-ptr fields modeled as `u64` (identical 8/8);
P5.2's generated mirror substitutes the boundary forms (`X|null`,
branded handles, `string`), which lower to the same layout.

Phase Review (2026-07-23): 0 CRITICAL, 0 MAJOR, 2 MINOR (no fix — a
tautological field-name parse check, and generic chain field names that
are compliant). The proof was verified real by execution: a corrupted
language field (`i32`→`i64`) is caught with a precise per-field message;
absent `cc` fails; the two sides are independent (real C compiler vs the
same layout engine used in codegen); padding is non-trivial and
reproduced on both sides. Verification: `cargo test --offline` 215
green, zero warnings, sweep clean. **P5.1 COMPLETE.**

## P5.2a — bindgen + ingestion + boundary types: COMPLETE (2026-07-23)

New `bindgen/` crate (std-only): `interop.h` → `corpus/interop/
interop.generated.d.ts` per the Q13 rules, with a byte-identical
regeneration test (§12.2). The checker ingests the mirror as a global
ambient scope (parser `dts:true` for mirror files via
`SourceFile::ambient`): `interface`→branded handle (nominal reference
class, non-cross-assignable), `declare class`→boundary struct (value
class bypassing the C2 field whitelist), `type`→alias, `declare
function`→a foreign symbol (`hir::ForeignFn`, `Callee::Foreign`, signature
only — no lowering), `declare const`/`enum`→ambient constants.

Boundary rule implemented: value-class-with-null resolves to
`Nullable(Class)` and `object`/`object | null` are legal **only** while
`in_boundary` is set (mirror pass); in ordinary program source both
stay S011. Branded-handle non-cross-assignment is nominal.

Deviation from the Q13 suggestion, recorded: handles use a `never`
phantom-property brand, not `unique symbol` — tsc (TS1332) forbids
`unique symbol` on interface members; the `never` form is tsc-clean and
non-cross-assignable. interop.h contains no flag-set typedef, so the
`u64`-alias path is tested via an inline mirror, not the committed one.

Verification (orchestrator-reproduced): `cargo test --offline` 229
green, zero warnings; `npx tsc -p tsconfig.json` clean (invariant 5 —
the mirror + a using program `corpus/interop/use-interop.ts` type-check
under stock tsc and this checker); regen test fails on drift both ways.

Phase Review (2026-07-23): 0 CRITICAL, 0 MAJOR, 2 MINOR. The review
machine-confirmed no weakening of ordinary-code checking — every
boundary relaxation is reachable only through `dts:true` mirror files,
and `Struct|null` / `object` / ingested `interface`/`declare function`/
`type` all stay rejected in ordinary `.ts`. MINOR 1 fixed: removed the
one `unreachable!()` in library code (`check/expr.rs`, contextual-lambda
param) by binding the ident in the guard. MINOR 2 (whether a bodyless
`declare class` in ordinary program source should be a reject-corpus
entry) is a language-design question deferred beyond P5 — pre-existing
behaviour, no soundness impact. **P5.2a COMPLETE.**

## P5.2b — foreign-call lowering, both tiers: COMPLETE (2026-07-23)

`Callee::Foreign` lowers to a real C-ABI call in both tiers. Dev JIT:
the header symbol is `Linkage::Import`, and `codegen/build.rs` compiles
`corpus/interop/interop.c` → `libinterop.a` whose addresses `jit.rs`
registers with the JITModule (alongside new runtime symbols
`sub_rt_str_data`/`array_data`/`cb_bind`/`cb_trampoline`). Ship C:
`#include "interop.h"` + direct calls; `run_c_aot`/`run_aot` add
`-I corpus/interop` and compile `interop.c` into the link. One C
implementation serves both tiers.

Marshaling (Q13): `string` ↔ `(const char*, size_t)`; `T[]` ↔
`(const T*, size_t)`; handle / `object|null` ↔ one pointer; `Struct|null`
↔ nullable struct pointer (address-of storage, or 0). Chain-slot
address-of: a guarded `store_val` arm writes the struct's storage
address into a `Nullable(value class)` slot — reachable only through a
C7 boundary-only type, never from ordinary code. Callback trampoline:
`sub_rt_cb_trampoline` bridges the language `SubFn{code,env}` /
`(ctx,env,args)` convention to the C `(fnptr, void* userdata)`
convention via a per-callback `CallbackBinding{ctx,code,env,userdata}`
held in the Context; it delivers the script's real userdata and the
Context is captured per-binding, not global.

`corpus/interop/interop.c`: deterministic, headless, libc-only —
create walks the chain, setLabel stores, setLogger fires the callback
with the label, submit sums the `(ptr,count)` commands and fires the
callback. Defines the P5.3 goldens.

The five patterns each pass through a real foreign call, byte-identical
across tiers (`codegen/tests/interop.rs`, 6 differential tests): handle
create/retain/release, string label, `(ptr,count)` sum, chain-slot
address-of (constructor and `next =` forms), all five composed.

Phase Review (2026-07-23): 0 CRITICAL, 1 MAJOR, 3 MINOR. Sound and
byte-identical across tiers **on arm64** (the gate machine, the sole
ship target, and the run set's platform).

- MAJOR M1: the JIT by-value boundary-struct marshaler was hardcoded to
  AAPCS64, so on a non-arm64 dev host (x86-64 SysV / Win64) it would
  mis-marshal a >16-byte struct (e.g. `SubCallbackInfo`, 24 B) — silent
  dev-JIT ≠ ship-C. Contract scoped (compiler block Rev 10, §12.3a):
  dev-tier boundary-struct-by-value marshaling is **arm64-only for
  now**; on a non-arm64 target it is now a **loud codegen error**
  (`boundary_struct_by_value_supported`, gated only on the by-value
  path — scalar/pointer/`(ptr,len)`/`Struct|null`-pointer/`object|null`
  args stay target-neutral). x86-64 SysV / Win64 marshaling is a
  tracked follow-up (untested on this arm64 machine; a fail-loud
  restriction beats untested ABI code). The ship tier is arm64-only C
  emission where the C compiler marshals correctly, so shipped code is
  unaffected.
- MINOR m1 fixed: the trampoline now checks `ctx.trapped()` before
  invoking script, so a trap stops the run even if a callee fires the
  callback more than once. m2 (chain-slot / userdata lifetime) and m3
  (transient `alloc_str` rooting) are by-design under Q13 / invariants
  2 & 6 — recorded, not changed.

Verification (orchestrator-reproduced): 236 tests green, zero warnings;
6 interop differential tests byte-identical; the 24-entry standing gate
byte-exact; goldens untouched; the arch classifier accepts aarch64 and
rejects x86-64 SysV / Win64. **P5.2b COMPLETE.**

### Follow-ups tracked (beyond P5)

- Dev-tier boundary-struct-by-value marshaling for x86-64 SysV and
  Win64 (target-aware ABI; §12.3a). Blocked on a non-arm64 host to
  verify against.

## P5.3 — both-tier interop corpus slice + gate: COMPLETE (2026-07-23)

Six headless accept entries against the generated mirror, each
self-creating its handle via `subDeviceCreate` (Q16), deterministic,
`print`-terminated:

| Id | Pattern | Golden |
|---|---|---|
| a25-interop-chain | intrusive extension chain (depth 3) | `3` |
| a26-interop-array-pair | `(pointer,count)` `u32[]` view (sum 100) | `100` |
| a27-interop-string-view | length-carrying string label (12 bytes) | `12` |
| a28-interop-callback | callback + `as`-narrowed userdata, fired twice | `8` |
| a29-interop-handle | opaque handle create/retain/release | `ok` |
| a30-interop-compose | all five composed | `20` |

The standing gate (`golden.rs`) floor is 24 → 30; it derives its set
from `corpus/accept/`, asserts dev-JIT ≡ ship-C-AOT ≡ golden byte-exact
per entry with `compared == golden_ids.len()` (no silent skip). The
harness ingests the mirror as an ambient `.d.ts` for entries that call
`subDevice…` and links `interop.c` in both tiers. The P1 checker gate
(`corpus_accept.rs`) count is 23 → 29 (single-file entries), so a25–a30
are rule-accepted, not merely tsc-clean.

Phase Review (2026-07-23): 0 CRITICAL, 0 MAJOR, 2 MINOR (non-blocking:
a29's `ok` weakly discriminates a pure-lifecycle pattern; the
mirror-ingestion predicate is a `subDevice` substring match, safe in
both directions). The review hand-derived every golden from `interop.c`
and confirmed none is vacuous, and verified the freeze rests on real
cross-tier agreement by experiment: corrupting a golden fails BOTH
tiers, and perturbing the shared C callee moves both tiers in lockstep
on exactly the callback-firing entries — proving both tiers re-derive
and execute the real callee every gate run, not a one-time capture.

Verification (orchestrator-reproduced): 236 tests green, zero warnings;
`npx tsc -p tsconfig.json` clean (invariant 5 — the six entries + the
mirror); 30-entry standing gate byte-exact on both tiers; interop
goldens present and correct. **P5.3 COMPLETE.**

## P5 — C-header binding vertical slice: COMPLETE (2026-07-23)

All of §12 discharged: the neutral synthetic header (P5.1); invariant 1
machine-verified against the platform C compiler's `offsetof`/`sizeof`/
`_Alignof` for every mirrored struct (P5.1); the bindgen mirror
generator with byte-identical regeneration + `.d.ts` ingestion +
boundary type system (P5.2a); foreign-call lowering in both tiers with
marshaling, chain-slot address-of, and the callback trampoline (P5.2b);
and the headless five-pattern corpus slice with cross-tier-verified
goldens in the standing gate (P5.3). The language's founding purpose —
zero-marshaling C-ABI interop — is now proven from layout through
execution to executable corpus definition, on the arm64 ship target,
with the dev-tier boundary-struct-by-value marshaling scoped to arm64
and fail-loud elsewhere (§12.3a).

## P5 extension — typed-slice facade for primitive arrays (a31, 2026-07-24)

Demonstrates zero-copy passing of a primitive-typed array to a C API via
a typed `(pointer, count)` descriptor — the generic facade that lets a
script hand a primitive array to a C function with no element copy (a
byte-size or units conversion, when the target API wants one, is done
host-side in the facade; the language passes the array's own backing
store). Extends the P5.3 slice beyond the original u32.

- `corpus/interop/interop.h`: four borrowed slice descriptors
  `SubSlice{F32,I32,F64,I64}` = `{const <T>* items; size_t count;}` and
  facade consumers `int32_t subSliceChecksum<T>(SubSlice<T>)` that read
  every element (the read proves the borrow reaches subscript's array
  storage). `interop.c` implements them (i32-wrapping rolling hash;
  float elements cast to `int32_t`).
- Mirror regenerated by `bindgen` (byte-identical test green): the
  descriptors absorb into `T[]`, so the mirror gains
  `declare function subSliceChecksumF32(data: f32[]): i32;` etc. The
  bindgen already maps every primitive element (`lang_scalar`), so no
  bindgen change was needed.
- `corpus/accept/a31-interop-primitive-slices.ts` builds `f32[]`/`i32[]`/
  `f64[]`/`i64[]` and passes each zero-copy; golden
  `31810\n10260\n3300\n216514\n` (independently re-derived from the
  element values — genuine zero-copy read). Runs in the standing gate on
  both tiers: dev-JIT ≡ ship-C-AOT ≡ golden, byte-exact. Element types
  now demonstrated end-to-end: u32 (a26), f32, i32, f64, i64.
- `codegen/src/cemit.rs`: the ship-C array-pair marshaler was hardcoded
  to `SubBufferView`; now `interop_array_pair_c_struct(elem)` maps the
  element type to the header's descriptor struct name. The JIT tier was
  already element-generic.

Phase Review (2026-07-24): 0 CRITICAL, 0 MAJOR, 1 MINOR. Sound — no
silent cross-tier ABI divergence on any reachable element type (the
16-byte `{ptr,size_t}` descriptor ABI is element-type-independent; both
tiers execute the same linked `interop.c`).

**Known limitation (MINOR, non-blocking).** `interop_array_pair_c_struct`
covers exactly the element types the committed header declares a
descriptor for (u32/f32/i32/f64/i64). `bindgen` can also produce `u64[]`/
`boolean[]` slice params if a header declared such a descriptor; for
those the JIT marshals generically but ship-C `emit_c` returns a **loud
`Err`** (never a silent mis-marshal or panic). Unreachable from the
committed corpus; a header adding a slice of another element type must
add the matching cemit mapping, and the standing both-tiers gate catches
the mismatch loudly (ship-C run fails). A registry-derived struct name
(instead of the hardcoded table) would remove the manual sync step —
future robustness, not a defect.

## P6.1 — libclang C frontend: COMPLETE (2026-07-24)

`bindgen` crate's narrow fixture parser (`cparse`) replaced by a
libclang frontend (`bindgen/src/clangfe.rs`, `clang-sys = "=1.8.1"`
feature `runtime`, offline-clean; libclang located via `LIBCLANG_PATH`
then existence-guarded OS defaults, missing → clean `Err`). It walks the
translation unit into the same `CField`/`Decl` IR `emit.rs` consumes,
preserving stdint spellings and const/pointer flags exactly (only
`_Bool`→`bool` / `void` normalized).

Gate: **byte-identical regeneration** of `corpus/interop/
interop.generated.d.ts` via libclang (`emit.rs` untouched, committed
mirror unchanged) — proving the new frontend is a superset of `cparse`
on the existing fixture. New neutral fixture `corpus/interop/prodfix.h`
(macros object/function-like, visibility + nullability attributes, doc
comments, `static const`, `typedef uint64_t SubFlags` + members, nested
structs, fn-ptr typedef, intrusive chain) parses; `bindgen/tests/
prodfix.rs` (6 tests) asserts non-vacuous extraction (macros stripped
from signatures, attributes ignored, flag members at correct bit
values). `cparse` kept as the shared IR module + a `#[cfg(test)]`
fixture parser; `generate()` uses libclang exclusively.

Verification (orchestrator): `cargo test --offline` 244/0, zero
warnings; mirror `git diff` empty; sweep clean. `cparse.rs`
`#endif`-diagnostic from the harness linter is a false positive
(`#ifndef`@21/`#define`@22/`#endif`@89 balanced; `clang -fsyntax-only`
exit 0).

Phase Review (2026-07-24): 0 CRITICAL, 0 MAJOR, 5 MINOR. FFI memory-safe
(RAII dispose of index/TU in correct order on every path, all
strings/diagnostics/eval-results freed, no use-after-free, no
panic/UB on parse errors, accurate SAFETY comments); libclang location
robust; byte-identity robust by probe, not luck; production-C parsing
genuine. Trivial MINORs fixed (infallible `unwrap`→`expect`; two doc/
comment nits). **P6.1 COMPLETE.**

**Carried into P6.2 (from the review's key robustness finding).** The
*emitter* (`emit.rs`, unchanged by P6.1) maps only stdint typedef
spellings via `lang_scalar`; on an arbitrary real header a field written
as a raw C builtin (`int`/`unsigned int`/`long`/`char`), an anonymous
inline struct, or a double pointer would currently be emitted **literally**
into the `.d.ts` (silent invalid mirror). Before P6.2/P6.3 feed
`generate()` an arbitrary header, `emit.rs` must (a) map the raw builtins
with a defined language equivalent and (b) **fail loud (`Err`) on any
unmapped base spelling**, never emit a literal. Not reachable today
(`generate()` only runs on stdint-only `interop.h`), but a required P6.2
deliverable.

Next: P6.2 — descriptor-embedded `(count,pointer)` arrays, flag typedefs
end-to-end, untyped-data facade; plus the emit.rs fail-loud/builtin
mapping above.

## P6.2 — production-C binding shapes: COMPLETE (2026-07-24)

Three shapes a production header uses that the P5 fixture lacked, each a
both-tier corpus entry with a committed golden (dev-JIT ≡ ship-C-AOT ≡
golden, byte-exact):

- **Descriptor-embedded `(count,pointer)` arrays** (a32, golden `105`):
  a `size_t <n>Count` field immediately followed by a `const T* <n>`
  field (count-first, contiguous — §13.2) maps the pointer to `T[]`,
  count elided; both tiers fill (count@i, ptr@i+8) from one `T[]`,
  byte-exact against the real C `offsetof`.
- **Flag typedefs end-to-end** (a33, golden `1\n0`): `typedef uint64_t X;
  static const X X_A = …;` → `type X = u64;` + folded `declare const`
  members; combined with `|` (Q18 true 64-bit), passed to a foreign
  bit-check.
- **Untyped bulk-data facade** (a34, golden `1691665087`): a
  `const void* data, size_t size` API + a typed slice facade (count→bytes)
  binding a `f32[]` zero-copy — the writeBuffer-shape path.

**Emitter hardened** (`bindgen/src/emit.rs`, `Result`-threaded): raw LP64
builtins mapped (`int`→i32, `unsigned int`→u32, `long long`→i64,
`unsigned long long`→u64; `float`/`double`); any base that is neither a
mapped scalar/builtin nor a registered named type **fails loud** (`Err`,
never a literal) — verified by probe (anonymous struct, `T**`, bare
`char`, lone scalar pointer, `long`, `unsigned long` all error cleanly).
`interop.h` regenerates byte-identical.

Phase Review (2026-07-24): 0 CRITICAL, 1 MAJOR, 3 MINOR. Committed corpus
sound and fully gated on both tiers. Fixed:

- MAJOR M1: the embedded-array recognizer paired count/pointer by name
  *anywhere* in a struct, but both lowerings reconstruct
  count-first-immediately-before-pointer — so a pointer-first or
  non-adjacent-count header regenerated a byte-identical mirror yet
  mismarshaled on both tiers with no fail-loud. Recognizer narrowed to
  the exact count-first-adjacent shape (§13.2 narrowed to match);
  everything else leaves the pointer unmapped → fails loud. Verified: a
  3-field ptr-before-count and a non-adjacent-count struct now `Err`
  cleanly; count-first-adjacent and the standalone 2-field descriptor
  unchanged.
- MINOR m1: `long`/`unsigned long` (LP64-ambiguous; 32-bit on Windows
  LLP64) dropped from the builtin map → bare `long` fails loud, so a
  header must use `int64_t`/`long long`.
- MINOR m2: a flag `static const` value > 2^53−1 (unrepresentable via the
  f64 literal channel, C3 open item) now fails loud at bindgen instead of
  silently truncating.
- MINOR m3 (no change): ambient-literal→u64 folding is scoped to `.dts`
  mirror files only; ordinary program `const` unaffected (confirmed by
  the code path + suite).

Verification (orchestrator): `cargo test --offline` 260/0, zero
warnings; goldens unchanged; regen byte-identical; full-repo reference
sweep clean; every fail-loud case reproduced independently. **P6.2
COMPLETE.**

Next: P6.3 — async callback model (§13.3), scaled offsetof over a
production-shape fixture (§13.4), generic `--header` CLI + local
real-header capability demo (§13.5).

## P6.3 — async, scaled layout, generic CLI: COMPLETE (2026-07-24)

- **Async deferred-fire callback (§13.3).** interop.h gains
  `SubCompletionInfo` + `subDeviceOnComplete` (stores the callback-info,
  returns without firing) + host-driver `subDevicePump` (fires it
  later). The runtime `CallbackBinding` is Context-held (`Box`-stable),
  so the binding stored in C at registration survives the intervening
  work and the later pump — Q13 "userdata must outlive the registration"
  holds by construction. Corpus a35 (golden `0\n0\n60`) proves the
  deferred fire on both tiers (dev-JIT ≡ ship-C-AOT). `collect()` between
  register and pump keeps a live-local userdata (shadow-stack root);
  `unsafeDelete` of the userdata before pump traps in dev, undefined in
  AOT (the general Q6 policy — the deleting program is incorrect by Q13).
- **Scaled layout proof (§13.4).** The offsetof suite grew 8→**36**
  neutral `Sub`-prefixed structs (chains, by-value string views, two-level
  nesting, flag-typedef fields, embedded-array raw layouts, varied
  padding). Every one matches the platform C compiler's
  `offsetof`/`sizeof`/`_Alignof` — invariant 1 discharged at production
  scale. Review independently reproduced four complex structs and
  confirmed a wrong layout fails (non-tolerant).
- **Generic CLI (§13.5).** `subscript-bindgen --header <path>` (bin
  renamed) binds any header to a mirror on stdout; an unmapped construct
  fails loud (exit 1, empty stdout, names the construct). cli.rs tests.

**Local real-header capability demo (uncommitted, host-agnostic).** The
CLI was pointed at a real production low-level GPU C API core header
(~20k lines) on the machine: the libclang frontend parsed it fully
(~1200 structs, ~640 functions, ~290 enums, ~650 fn-ptr typedefs, ~1100
macros); the emitter then failed loud (correctly) on the first construct
outside the mapped subset (a two-level flag alias; the header also has
unions), and an `offsetof` spot-check of five of its real all-scalar
structs matched the platform C compiler exactly. No tracked file names
or contains the external API, the header, or its generated mirror
(reference sweep clean); the demo artifacts were `$TMPDIR`-only.
Invariant 4 intact.

Phase Review (2026-07-24): 0 CRITICAL, 0 MAJOR, 2 MINOR (non-blocking):
(m1) the `CallbackBinding.code` raw pointer is not reload-epoch-guarded,
so a dev-tier hot reload between register and pump would call swapped-out
code — a pre-existing P5.2b property the deferred fire widens; recorded
as a follow-up (a registered callback does not survive a reload). (m2)
cosmetic `bindgen` name in an error prefix / doc comment after the bin
rename — own tool name, no hygiene impact. **P6.3 COMPLETE.**

## P6 — production-C-header interop: COMPLETE (2026-07-24)

The toolchain binds an arbitrary production C header while the language
stays host-agnostic (invariant 4) and the repo names no external API
(reference sweep zero). P6.1: libclang frontend (byte-identical mirror
regen). P6.2: descriptor-embedded arrays, flag typedefs, untyped-data
facade, and a fail-loud emitter (no silent invalid mirror). P6.3: async
deferred-fire callbacks, a 36-struct offsetof proof, and a generic
`--header` CLI proven locally on a real ~20k-line GPU C API. Verdict on
the founding question: binding a real production C API (WebGPU-class) has
no language-level blocker — the committed proof is the neutral
production-scale fixture on both tiers; pointing the CLI at a specific
real header is a local step it now supports, with any unmapped construct
failing loud.

### Known follow-ups (beyond P6)
- Reload-epoch-guard the callback binding's `code` pointer (m1).
- Two-level flag aliases and `union`/bitfield handling (constructs some
  real headers use that the mapped subset fails loud on today).
- x86-64 SysV dev-JIT boundary-struct-by-value marshaling (§12.3a, still
  arm64/Win64 only).
