<!-- §183 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 183. A host-facing definition has its declared type

*(Added 2026-10-09.)* Origin: a documentation audit of 2026-10-09
found that the C tutorial's `boolean` parameter row disagrees with
`program.h`. On 2026-10-09 the owner selected this section.

Problem: since §179 (`3de68c37`), a script whose host export has a
`boolean` parameter fails the C AOT build. Measured with the release
CLI on 2026-10-09 (`codegen/` as at `3de68c37`):

```ts
export function step(dt: f32, paused: boolean): void { if (!paused) { print(`${dt}`); } }
export function main(): void {}
```

`program.h` declares `void subscript_export_step(subscript_rt_context*
ctx, float a0, bool a1);`. `program.c` includes `program.h` and defines
`void subscript_export_step(subscript_rt_context* ctx, float a0,
int32_t a1)`. The C compiler reports `conflicting types for
'subscript_export_step'`, and `subscript build` exits 2.

The header takes the parameter type from `ctype`, which reads the §179
boundary kind record. The wrapper takes it from `value_ctype`, which
maps a `bool` value to `int32_t`. This breaks §179 rule 1: the C
emitter derives a boundary C type again from another type. No test
builds a host export with a `boolean` parameter, so the gate did not
see it.

### 183.1 Rules

1. **Declared type.** Each C definition that the emitter writes and
   that a host declaration also names takes each parameter and result
   type from the §179 boundary kind record. A host export wrapper
   converts each parameter to its local carrier inside its body. The
   round-1 survey (`specs/tracking/s183-host-facing-definitions.md`)
   lists the definitions; the export wrapper is the only one whose
   types come from a script signature.
2. **The C compiler checks it.** `program.c` includes `program.h`, so
   an export wrapper with another type fails the C build. The C flags
   of the ship tier and of the tests make a definition that conflicts
   with its prototype an error on each supported compiler: Clang and
   GCC give an error by default; MSVC gives warning C4028 (parameter
   type) and C4029 (parameter count) *(docs)*, so the flags add
   `/we4028` and `/we4029`.
3. **Each tier.** A host export with a parameter of each boundary
   scalar kind gives the same value in the dev JIT and C AOT.

### 183.2 Acceptance

1. Red first, at the contract pin: a test that derives the boundary
   scalar kinds from the §179 record (not from a list in the test),
   and for each kind builds a C AOT program whose host export takes a
   parameter of that kind, calls it from a C host with two values
   (for `bool`: `true` and `false`), and compares the printed values
   with the dev tier. The `bool` case fails at the pin with the
   conflicting-types error.
2. A firing control: the test reports a kind whose wrapper type the
   test changes (in the test, not in the emitter) to another C type.
3. State the cost of the test (core principle 15).
4. Goldens: no `.expected` output moves. The C tutorial's boundary
   type table names the declared type of `boolean`.

### 183.3 Sections this one amends

- §179 rule 1: the export wrapper definition reads the record too.
- §59 rule 2: the wrapper parameter has its boundary C type, not the
  internal C type.

### 183.4 Open

1. **A bound callback is stored through a cast.** The binder admits
   one callback shape, `void Callback(StringView, void*, void*)`
   (`bindgen/src/emit.rs`, `validate_callback_shapes`), so no scalar
   kind reaches a callback parameter. The runtime defines the
   trampoline with its own string-view type, and
   `codegen/src/cemit/marshal.rs` casts its address to the host
   typedef, so the C compiler does not compare the two aggregates. The
   binder checks the host view at bind time, but the callback record in
   the HIR and the LIR keeps only the typedef name, not the aggregate
   spelling, so the emitter cannot write a layout assertion for it. No
   good-faith header reaches a mismatch today, because the binder
   rejects another view shape.
