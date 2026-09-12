<!-- §62 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 62. R33 — an alignment override on `@CStruct` value classes

Origin: downstream request R33, 2026-08-22, at pin `4313dcf`. The
downstream uploads `FixedArray<T, N>` of `@CStruct` classes to GPU
buffers with no encoder. That path is correct only when the C layout
equals the WGSL layout. WGSL aligns `vec3<f32>` and `vec4<f32>` to 16
and `vec2<f32>` to 8; C aligns `{ x: f32; y: f32; z: f32 }` to 4. No
spelling today raises the alignment of a value class. Owner decision
of 2026-08-22: the language gains a class-level override.

Measurements at the pin, on this host:

1. The call form `@CStruct({ align: 16 })` fails with S100 "the only
   decided decorators are the ambient `@CStruct` and `@Descriptor`"
   (`compiler/src/check/mod.rs`, `class_decorators`: it matches
   `Expr::Ident("CStruct")` only). The class is then not a value
   class, and a field of that type fails the value-class whitelist.
2. Two sites compute the class layout and both take the alignment as
   the maximum field alignment: `codegen/src/layout.rs`
   (`class_layout`, feeds `Repr::Agg` and `StructLayout`) and
   `compiler/src/check/layout.rs` (the aggregate-limit validator).
3. The ship tier emits `typedef struct {name} { ... }` with no
   alignment attribute (`codegen/src/cemit.rs`, `emit_one_typedef`).
   Both tiers compile C as C11 (`-std=c11`, `/std:c11`).
4. Apple clang 21, C11, with `_Alignas(16)` on the first field:
   `Vec3f` 16/16 at offsets 0,4,8; `Particle { pos, vel }` 32/16 at
   0,16; `Mixed { a: f32; p }` 32/16 at 0,16; `Mat3x3f` 48/16 at
   0,16,32; `Vec3f a[4]` stride 16; `Vec2f` with `_Alignas(8)` 8/8.
   These equal the downstream's table and the WGSL offsets.
5. Stock `tsc` 5.9.2 with the overload in 62.2 accepts
   `@CStruct({ align: 16 })` and rejects `align: 3` (TS2322), an
   unknown key (TS2353), and `@Descriptor({ align: 16 })` (TS2554).
6. Heap objects start at a 16-aligned payload (`HEADER_SIZE` 16,
   allocation alignment 16 on both the host allocator and the ship
   arena), so a 16-aligned field inside a reference class is aligned
   in memory on both tiers.

### 62.1 Rule

1. `@CStruct({ align: N })` declares a value class whose alignment is
   `N`. `N` is an integer literal in `{2, 4, 8, 16}`. The size is the
   natural size rounded up to `N`. Field offsets do not change.
2. `N` must be greater than or equal to the natural alignment. A
   smaller `N` is an S100 whose message names both numbers: the
   requested `N` and the natural alignment.
3. A class with the override is an ordinary value class everywhere
   else: a field of another value class or of a reference class, a
   `FixedArray` element, a local, a parameter, a module global. The
   containing aggregate and the `FixedArray` stride use the overridden
   size and alignment, as C does.
4. Both tiers carry the same numbers. The dev tier's `Repr::Agg
   { size, align }` and the ship tier's C struct agree, and the
   `offsetof` proof (§12.3, `codegen/tests/offsetof_layout.rs`)
   asserts `sizeof` and `_Alignof` for a class with the override.
5. The options literal accepts the key `align` only. Any other key, a
   non-literal value, a value outside the set, a second argument, an
   empty literal, and the call form on `@Descriptor` are each an S100.
6. A generic value-class template carries the override into every
   instantiation.
7. Copy-on-assign, field initializers (§57), constructors, and the
   value-class whitelist do not change. The override is layout only.
8. Out of scope: a per-field `align` or `size`, an alignment above 16,
   and an alignment imported from a C header through `subscript bind`.
   A mirror-ingested boundary struct never carries the override, so
   no overridden class crosses the host ABI by value.

### 62.2 Changes by site

- `prelude/lang.d.ts`: one overload beside the existing declaration,
  so stock `tsc` accepts the call form:

  ```ts
  declare function CStruct(options: { align: 2 | 4 | 8 | 16 }):
    <T extends abstract new (...args: never[]) => object>(
      target: T, context: ClassDecoratorContext) => T;
  ```

- `compiler/src/hir.rs` `ClassDef`: one optional field, the alignment
  override, `None` for every class without it.
- `compiler/src/check/mod.rs` `class_decorators`: accepts
  `Expr::Call` with callee `CStruct` and one object-literal argument;
  reads `align`; reports the 62.1 rule 5 rejections at the decorator
  span. `GenericClass` carries the override (rule 6). The rule 2 check
  runs where the class layout is known.
- `compiler/src/check/layout.rs` and `codegen/src/layout.rs`: the
  final alignment is `max(natural, override)`, and the size rounds up
  to it. `StructLayout.align` reports the overridden value.
- `codegen/src/cemit.rs` `emit_one_typedef`: when the class has the
  override, the first field declaration carries `_Alignas(N)`. C11
  gives the struct that alignment and rounds `sizeof` (measured, 62
  item 4, clang). MSVC `cl /std:c11` accepts `_Alignas` *(docs)*; the
  windows-msvc run confirms it.
- `codegen/tests/offsetof_layout.rs`: the proof adds classes with the
  override. The C side declares the same structs with `_Alignas` in
  the probe source; `corpus/interop/interop.h` does not change (rule
  8).
- `compiler/src/language_reference.rs`: one sentence on the override
  in the value-class entry; `generated-docs/` regenerates.

### 62.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 in 62 item 1, recorded
(this host, exit 1).

1. `corpus/accept/a141-cstruct-align.ts` + `.expected`: `Vec3f`
   with `align: 16` and three `f32` fields; `Mixed { a: f32; p: Vec3f
   }`; a `FixedArray<Vec3f, 4>` field; values copied on assignment
   and printed through field reads. Golden from the dev JIT; ship
   byte-identical.
2. `corpus/reject/r135-cstruct-align-below-natural.ts`: `align: 2`
   on a class whose natural alignment is 4; S100 with both numbers.
3. `corpus/reject/r136-cstruct-align-not-in-set.ts`: `align: 3`;
   S100.
4. `corpus/reject/r137-descriptor-align.ts`: `@Descriptor({ align:
   16 })`; S100.
5. Unit tests in the same commit: `value_class_layouts` reports
   (16, 16) for `Vec3f`, (32, 16) with `p` at 16 for `Mixed`, and
   (48, 16) for a three-`Vec3f` class; the emitted C typedef carries
   `_Alignas(16)` on the first field; the `offsetof` proof covers the
   override classes; an unknown key and a second argument are each
   S100.
6. Counts: accept `.ts` 139 → 140; `.expected` 140 → 141; rejects
   130 → 133. The generated docs regenerate.
7. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.
