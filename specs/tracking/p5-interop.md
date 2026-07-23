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
