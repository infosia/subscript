<!-- §179 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 179. A boundary value has its C ABI form

*(Added 2026-10-08.)* Origin: `compiler.md` §178.3, items 1–4. On
2026-10-08 the owner selected this section. The measurement round at
`261f9ce2` is `specs/tracking/s179-boundary-abi-measurement.md`.

Problem: values that cross the C boundary, and value-class layouts,
do not always have the form that the platform C compiler gives them
(invariants 1 and 4). The measurement compared 636 script checks in
the dev JIT and C AOT with C callees that the platform C compiler
built (Apple arm64). At the pin, 186 values are wrong, 40 checks are
refused, and 3 C AOT layouts differ from C. Five causes account for
every failure:

| Class | Failing form | Tier | Wrong or refused | Example (good faith) |
|---|---|---|---:|---|
| N | `int8_t` and `int16_t` register arguments are not sign-extended | JIT | 4 | `f(-31)` reads another value |
| F | `_Float16` crosses as a number, not as its bits; in a general register in the JIT | JIT, C AOT | 101 | `f(1.5)` reads another value |
| H | a struct with `_Float16` leaves is not an HFA | JIT | 76 | `f(new H2(1.5, 2.5))` |
| B | a `bool` field is `int32_t` in C AOT | C AOT | 5, and 3 layouts | a return `{bool; i32; bool}` with nonzero padding reads `true` for `false`; a script-only `@ValueType` with two booleans is 8 bytes, not 2 |
| R | an `f32` or `f64` HFA return is refused | JIT | 40 | `const v = returnsVec2();` fails in the dev JIT; C AOT works |

With one prototype per class, every failure goes, and the 335 accept
entries agree in both tiers.

### 179.1 Rules

1. **One boundary kind record.** Each boundary type kind has one
   record: its C type, its storage size and alignment, its native
   carrier (general register, SIMD register, or memory), its caller
   extension (signed, unsigned, or none), and its HFA leaf class. The
   binder (`map_use`), the dev JIT native signature, the C emitter, and
   the layout builder each read that record. None of them derives
   these facts again from another type.
2. **Caller extension (N).** The dev JIT extends each integer argument
   narrower than 32 bits, and a `bool` argument, as the target C ABI
   requires the caller to do (signed for signed types, unsigned for
   unsigned types and `bool`). A native return narrower than 32 bits is
   read at its own width.
3. **Half bits (F).** A `_Float16` value has two-byte storage that
   holds its bits. At each boundary crossing, in both directions and
   in both tiers (an argument, a return, a struct field, a pointer
   writeback, a completion result), the bits are copied, never
   converted as a number. The native carrier of a `_Float16` scalar
   is a SIMD register where the target ABI says so.
4. **Half HFAs (H).** A `_Float16` leaf is a floating-point leaf for
   HFA classification. The §178 rule 13 allocator places a `_Float16`
   HFA argument as it places an `f32` or `f64` HFA. A `_Float16` HFA
   return uses the SIMD return registers.
5. **HFA returns (R).** The dev JIT returns an `f16`, `f32`, or `f64`
   HFA in the SIMD return registers that the target ABI gives it. The
   §12.3a refusal of a floating-point HFA return is removed.
6. **Bool storage (B).** A `bool` field has one-byte storage in every
   value-class layout, in both tiers: a boundary struct, a script-only
   `@ValueType`, a nested class, an array element, and an async result
   carrier. A stored `bool` is 0 or 1. A local can have another width.
   A store of raw bytes into a value with `bool` storage
   (`Context.fromBytes`, stdlib §18.1 rule 5, and a §178 completion
   value) writes 1 for each nonzero `bool` byte, in both tiers.
7. **A total layout check.** For every value class that it emits, the
   C emitter emits `_Static_assert` lines that compare `sizeof`,
   `_Alignof`, and each field `offsetof` with the numbers of the layout
   builder (core principle 9: the C compiler derives one side, the
   layout builder the other). A layout that differs fails the C build.
   For a boundary struct that has a host header and that the tiers
   copy as bytes, the C emitter also compares the host header struct
   with the language struct: `sizeof` and each field `offsetof`. The
   fact that selects the comparison is the fact that selects the byte
   copy in the tiers (one derivation; a `FixedArray` field is copied as
   bytes). A boundary struct that the marshaler converts (a string
   view, an absorbed descriptor, a callback) has another layout by
   design; its marshal tests check it. A class with no host header has
   no host struct to compare.
8. **§178 rule 6** admits a struct with a `bool` field, because rule 6
   gives it the C layout; §178.3 items 1–4 are closed.
9. The interpreter has no foreign call (§68.7). Rule 6 applies to its
   value layouts if it builds C-visible bytes; otherwise it is
   unchanged.

### 179.2 Acceptance

1. Red first, at the contract pin: accept entry `a348` with a
   synthetic host library in `corpus/interop/`: a signed narrow
   argument, a `_Float16` scalar argument and return, a `_Float16` HFA
   argument and return, an `f32` HFA return, a struct return with a
   `bool` field and nonzero padding, and a script-only `@ValueType`
   with two booleans whose size is read through a C helper. Record the
   pin result in each tier.
2. A gate sweep (native C callees from the platform C compiler, both
   tiers, one module per tier) with one witness per class and position
   kind: each class N, F, H, B, R at a register position, at the last
   register position, and at a stack position, for arguments, returns,
   struct fields, pointer writebacks, and completion results. The
   §178 allocator sweep adds the `_Float16` and narrow integer kinds.
   A narrow unsigned witness passes a value whose upper register bits
   are not zero (for example a `u16` result narrowed to `u8`).
   Omitting any one rule's change must make the gate sweep fail. State
   its cost (core principle 15). The wider measurement sweep is not a
   gate test.
3. A unit test for rule 7 that builds a layout that differs from the
   emitted C struct, and reads the C compiler error.
4. Cost: the benchmark workloads and the interpreter corpus within 1.05
   of the pin. The C build of each benchmark workload with the rule 7
   assertions is within 1.05 of its C compile time at the pin.
5. Goldens: the generated API reference and corpus index move. No
   other `.expected` golden moves.

### 179.3 Open

From the measurement limits: the sweep does not measure NaN payloads,
signed zero, or subnormal values at each floating-point position (the
Phase Review probed them at a register, the last register, and a stack
position, with no failure). The other target ABIs (SysV, Win64) are
stated by their documents, not measured on this host *(docs)*.

A boundary struct with a C array field (`float m[4]`) passed by value
fails loud in both tiers (the dev JIT rejects the field type; the C
emitter builds an invalid initializer). It is older than this section;
a pointer to such a struct works.

A C callee that writes a byte other than 0 or 1 into a C `bool` (C
undefined behaviour) gives different results in the two tiers
(contrived).

### 179.4 Sections this one amends

- §12.3a: the floating-point HFA return refusal is removed (rule 5).
- §178 rule 6: a `bool` field is admitted (rule 8); §178.3 items 1–4
  are closed.
