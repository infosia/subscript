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

Next: P5.2b — foreign-call lowering in both tiers (dev JIT + ship C):
the callback trampoline (`SubFn{code,env}` ⇄ C `(fnptr, void* userdata)`),
the chain-slot address-of, `(ptr,count)` / string / handle / `Struct|null`
argument marshaling.
