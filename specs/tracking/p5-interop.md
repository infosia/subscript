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

Next: P5.2 — mirror generator + `.d.ts` ingestion + foreign-function
machinery.
