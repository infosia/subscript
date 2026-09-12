<!-- §16 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 16. P14 — narrow numerics (`i8`/`u8`/`i16`/`u16`/`f16`)

Owner decision 2026-07-25. Language rules: `collisions.md` Q23 (which
extends C3/C4/Q18). This is a type-system extension, not a stdlib
phase — no new library surface, five new sized types.

**Why now.** `bindgen`'s scalar map (`bindgen/src/emit.rs::lang_scalar`)
has no entry for `uint8_t`/`uint16_t`/`char`/`short`, and the emitter
fails loud on an unmapped construct, so **a production header with a
single byte field cannot be bound at all** — the same class of blocker
as the tracked two-level flag aliases. `f16` additionally unblocks the
GPU buffer formats mobile shaders consume (half-precision vertex
attributes, uniform blocks); the script builds the buffer, the device
does the half-precision math.

### 16.1 Scope

- Five ambient aliases in `prelude/lang.d.ts`: `i8`, `u8`, `i16`,
  `u16`, `f16` — aliases of `number`, exactly as C3's existing six.
- Layout: 1/1, 1/1, 2/2, 2/2, 2/2 (size/align), verified by the §12.3
  `offsetof` proof against the platform C compiler, not asserted.
- Both tiers: dev JIT (`codegen/src/lower/`) and ship C
  (`codegen/src/cemit.rs`) — one set of semantics, proven by the
  standing gate (§11).
- `T[]` of a narrow type is contiguous and zero-copy across the C
  boundary, exactly as the existing primitive slices (a31).
- `bindgen`: `int8_t`/`uint8_t`/`char`/`signed char`/`unsigned char`/
  `int16_t`/`uint16_t`/`short`/`unsigned short` map to the new integer
  types. **`char` signedness is platform-dependent**; map plain `char`
  only where the target's signedness is known, else keep failing loud
  rather than guessing.

### 16.2 `f16` is storage-only

Per Q23: `f16` declares fields, elements and boundary parameters and
converts with `as`; **arithmetic on `f16` operands is S014**. The
conversion (`f16`↔`f32`/`f64`) is one runtime implementation behind an
opaque `subscript_rt_*` symbol on both tiers — never an emitted compiler
builtin and never a direct `_Float16`/`__fp16` operation, for the
§11/`stdlib.md` §0.2 reason: the C tier's `_Float16` rounds in half
precision while `__fp16` promotes to `f32`, so an emitted half
operation is a silent dev-JIT ≠ ship-C divergence waiting to happen.
Conversion semantics: IEEE 754 binary16, round-to-nearest-even;
overflow to `±Infinity`; subnormals preserved; `NaN` preserved.

The C boundary type for `f16` is fixed by the mirror, not inferred:
`bindgen` maps a half-width float field to `f16` only for the
spellings whose in-memory representation is unambiguously binary16
(`_Float16`, `__fp16`, and a `typedef`ed 16-bit float); anything else
fails loud.

### 16.3 Gate (pre-registered)

1. Standing gate (§11) byte-exact on every entry, both tiers,
   including the new corpus entries.
2. `offsetof` layout proof (§12.3) green for structs mixing the narrow
   types with the existing ones — padding reproduced exactly.
3. `tsc -p tsconfig.json` zero errors, unchanged config.
4. Reject entries at pinned S-codes: bare `number` unchanged (C3);
   out-of-range narrow literals (C4); mixed-width arithmetic and
   bitwise without `as` (C3/Q18); **`f16` arithmetic (S014, Q23)**.
5. `f16` conversion round-trips pinned in a committed golden across
   the interesting cases: representable value, value rounded on
   narrowing, overflow to `Infinity`, subnormal, `NaN`, `-0`.
6. A production-shaped C header carrying `uint8_t`/`uint16_t`/`f16`
   fields binds through `bindgen` and passes its `offsetof` proof —
   the blocker this phase exists to remove is demonstrably removed.
7. Benchmarks (`benchmarks.md`): no ship-row regression.
