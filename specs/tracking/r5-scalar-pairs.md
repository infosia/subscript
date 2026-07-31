# §27 — scalar array-pairs at parameter position: evidence

Status: **landed and verified 2026-08-01** against `compiler.md` §27.
Origin: downstream request R5 (blocking its buffers+queue area).

**Diagnosis recorded first**: the request guessed `uint8_t` was
missing from the scalar table; it was not — `lang_scalar` already
mapped the full `stdint.h` set. The real gap was pointer-to-scalar
at parameter position routing to the named-type registry's fail-loud
path. §27 added the parameter-level `<name>Count`/`<name>` adjacency
rule instead of a scalar-table change.

## §27.3 evidence (reviewer-run)

1. `a96-interop-byte-pairs` byte-identical under both tiers: the
   consumer sum (256), the filled `u8[]` printed after the call
   (3/20/37/54/71 — the visibility pin), and the mutable `u16`
   variant (1000/1257/1514/1771).
2. Bindgen tests green: const/mutable/u16 pair collapse, lone
   scalar-pointer and non-adjacent pair keep the fail-loud error,
   and the stdint audit test pins that every `lang_scalar` spelling
   maps at a pair site (one table, as the request hoped).
3. The reviewer ran `subscript bind` live on a probe header with the
   downstream's three exact shapes; the mirror declares
   `data: u8[]` / `out: u8[]` / `index: u16[]` with
   `@subscript-c-scalar-pair` provenance records
   (const=true/false/true).
4. No previously tracked golden moved; gate 48 harnesses, 751
   passed, exit 0, read directly; `tsc` exit 0.

## Implementer decisions recorded

Parameter pairs accept exactly the camel-case `<name>Count`
spelling (the struct-level `_count` compatibility is untouched); a
dedicated `@subscript-c-scalar-pair` provenance record distinguishes
the two ABI parameters from by-value descriptors; the fixture's
filler patterns are fixed (`3 + i*17`-style bytes, `1000 + i*257`
shorts) for deterministic goldens.
