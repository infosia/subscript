<!-- §61 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 61. R32 — a wire-mapped alias in an entry signature

Origin: downstream request R32, 2026-08-16, at pin `1f875da`. The
downstream's first R30 consumer declared
`init(instance, device, format: GPUTextureFormat)` and the checker
rejected the export. R30's response invited the widening with
evidence; this is that evidence. The downstream does not ask for
plain string-literal unions in entry signatures — those have no C
representation, and the rejection stays right for them.

Measurements at the pin, on this host:

1. The report reproduces: a `CEnum` alias parameter on an exported
   function fails with S100 "exported function `init` has a
   string-literal union alias in its boundary signature". The
   rejection site checks every export for any string alias in
   params or return (`compiler/src/check/mod.rs`,
   `contains_string_alias`).
2. The wire channel exists in both directions at the bind
   boundary (R23/R24, §52): the ship tier validates through
   `validate_wire_alias` and `subscript_rt_trap_wire_enum`
   (`codegen/src/cemit.rs`); the dev tier calls the same trap
   symbol (`codegen/src/lower/func.rs`). Unknown wire values trap
   with the alias name (`corpus/trap/t48`, `t49`).

### 61.1 Rule

1. An exported function **parameter** can be a wire-mapped
   (`CEnum`) string-alias type. The checker keeps rejecting a
   plain (unwired) string alias in any exported signature, and
   any string alias in an exported **return** type.
2. R30's host-callable subset (§59.1) widens: every parameter is
   a boundary scalar, an opaque handle, or a wire-mapped alias.
3. At the host ABI, the parameter is the wire value as `int32_t`:
   the ship wrapper takes `int32_t`, and the dev tier accepts
   `EntryArg::I32`.
4. The crossing validates exactly as a bound-function return
   does (R23): a wire value outside the alias's table traps with
   the alias name and the parameter's position, **before the
   entry body runs**. A mapped value enters as the alias value.
5. In-script calls to the same exported function do not change:
   they never pass through the host wrapper, so no wire
   validation runs on the script-internal path.

### 61.2 Changes by site

- `compiler/src/check/mod.rs`: the export-signature check permits
  a wire-mapped alias in a parameter and keeps both remaining
  rejections.
- `codegen/src/lower/mod.rs`: `is_host_callable_export` and
  `entry_param_kind` admit the wire-mapped alias (kind `I32` at
  the ABI); the reload adapter validates the wire value and calls
  `subscript_rt_trap_wire_enum` on a miss, then returns without
  the entry call.
- `codegen/src/cemit.rs` (`emit_exports`): for a wire-alias
  parameter the wrapper body validates before the internal call,
  with a `pos_id` at the parameter declaration.
- `corpus/interop/wire-enum.c`: a drive hook on the a137 pattern
  (in the `.c` only; the header and the mirror must not move),
  with the same weak-fallback linking device.
- Test harnesses (`codegen/tests/golden.rs`, the trap-corpus
  support, `codegen/src/bin/capture.rs`): drive the new accept
  and trap entries on both tiers.

### 61.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 above, recorded (this
host, probe with the wire-enum mirrors, exit 1).

1. `corpus/accept/a140-wire-entry-param.ts` + `.expected`: an
   entry takes a `SubWireMode` parameter and a scalar; the host
   passes wire `23` (`"m1"`); the script prints the alias value
   and the scalar. Golden from the dev JIT; ship byte-identical.
2. `corpus/trap/t50-wire-entry-unknown-value.ts` + `.expected`:
   the host passes wire `12345`; the trap carries the alias name
   and the value, and the entry body never prints.
3. `corpus/reject/r134-plain-alias-entry-param.ts`: a plain
   string-literal union (no wire table) in an entry signature
   keeps the S100.
4. Unit tests in the same commit: the emitted wrapper for a
   wire-alias entry contains the validation and the trap call; an
   `EntryArg::I32` outside the table traps on the dev tier before
   the entry body runs; a wire-alias return type on an export
   still fails.
5. Counts: accept single files 138 → 139; accept source files
   140 → 141; goldens 139 → 140; rejects 129 → 130; trap corpus
   49 → 50. The generated docs regenerate byte-exact.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical; the
   wire-enum mirror regenerates byte-identically.
