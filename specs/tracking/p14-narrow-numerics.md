# P14 — narrow numerics (`i8`/`u8`/`i16`/`u16`/`f16`): COMPLETE (2026-07-25)

Contract: `specs/blocks/compiler.md` §16; language rules
`specs/blocks/collisions.md` Q23 (extending C3/C4/Q18).

## Why the phase existed

`bindgen`'s scalar map had no entry for `uint8_t`/`uint16_t`/`char`/
`short`, and the emitter fails loud on an unmapped construct — so a
production C header with a single byte field could not be bound at all.
`f16` additionally unblocks the half-precision GPU buffer formats mobile
shaders consume. Gate item 6 demonstrates the blocker is gone.

## What landed

Five ambient aliases behaving exactly as C3/C4/Q18 already specify
(bare `number` still rejected; `as` conversions with C truncation/
wrapping; no implicit conversion; mixed width requires `as`; contextual
literal typing). Layout 1/1, 1/1, 2/2, 2/2, 2/2, both tiers. Narrow
`T[]` is contiguous and zero-copy across the C boundary. bindgen maps
the unambiguous 8/16-bit C scalars.

**`f16` is storage-only.** Arithmetic with an `f16` operand is S014
("compute via `as f32`"); conversion is one runtime implementation
(`runtime/src/half.rs`) behind opaque `sub_rt_f16_from_f64` /
`sub_rt_f16_to_f64` on both tiers. Rationale (§16.2): the C tier's
`_Float16` rounds in half precision while `__fp16` promotes to `f32`,
so an emitted half operation is a silent dev-JIT ≠ ship-C divergence —
the same hazard `stdlib.md` §0.2 records for libm. A rejection can be
relaxed later on measured evidence; a silently diverging arithmetic
cannot be un-shipped.

Corpus: `a46` (the five types, conversions, wrapping), `a47` (mixed
narrow/wide `@CStruct` layout), `a48` (narrow zero-copy slices), `a49`
(the f16 conversion battery), `a50` (narrow-array callbacks + shifts);
rejects `r33`–`r37`.

## Phase Review (2026-07-25, fresh no-context, different model from the
## implementer)

Implementation by Codex `gpt-5.6-sol`; review by an independent
no-context agent. **2 CRITICAL, 2 MAJOR, 7 MINOR** — all fixed.

- **CRITICAL 1 — the ship tier computed wrong answers**, not merely
  diverged. Narrow *signed* array elements reached script callbacks
  unextended: `ArrElemKind::Int` collapsed all integers into one kind,
  so the runtime knew the element width but not its signedness, and the
  arm64 callee relies on the caller extending with the callee's
  signedness. `i8[].map(v => v as i32)` gave JIT `-128,3,-1,127` and
  ship `128,3,255,127`; `reduce` 1 vs 513; `findIndex` 0 vs −1. Before
  P14 the 1-byte case was only `boolean` (0/1), where the two
  extensions coincide. Fixed by carrying signedness through
  `ArrElemKind` (new `SignedInt` tag) to the runtime ABI dispatch.
  **The gate missed it because no corpus entry ran an Array-method
  callback over a narrow array** — `a50` now does, for four element
  types × five callback operations.
- **CRITICAL 2 — shift amount ≥ operand width diverged, and the wide
  widths were live C UB.** The dev tier (Cranelift) masks; the ship
  tier promoted the narrow operand to `int`, yielding 0, and for
  `i32`/`i64` produced *different results on re-runs of the same
  program*. Nothing in Q18 covered the case. Decided and recorded
  (Q18, owner 2026-07-25): **the amount is taken modulo the operand
  width on both tiers**, and a **literal** amount ≥ the width is
  rejected at compile time (S008). Masking over trapping because it is
  total, free (both ISAs mask in hardware), already the dev tier's
  behaviour, and what the TypeScript surface leads a reader to expect;
  "match C" has no meaning here because C's answer is undefined. The
  ship tier now emits the mask explicitly and routes left shifts
  through an unsigned carrier, which also removes the signed-overflow
  UB. This closes a defect that **pre-dated P14** for `i32`/`i64`.
- **MAJOR 1 — bindgen inferred plain `char`'s signedness from the
  generator host.** `char` is unsigned on `aarch64-linux-android` and
  arm64 Linux, both §11 ship triples, so the mirror was silently
  sign-wrong there. §16.1 required failing loud instead; plain `char`
  now does, naming `signed char`/`unsigned char` as the fix.
- **MAJOR 2 — bindgen silently mis-mirrored bitfields, unions and
  packed/over-aligned records.** The root defect pre-dated P14, but
  P14 *converted a loud failure into a wrong mirror* for narrow
  bitfields (previously they died on the unmapped scalar). All four
  constructs now fail loud with the record and member named.
- MINOR fixed: the `f16` literal bound now agrees with the `as` it
  models (reject only what overflows to infinity — `65505.0` is
  accepted and rounds to 65504); mixed-width bitwise says "bitwise",
  not "arithmetic"; `__fp16` no longer maps unconditionally (its format
  is target-dependent under `-mfp16-format=alternative` *(docs)*) —
  only `_Float16` maps; SAFETY comment added to the `F16` equality arm.

**Verified positively by the review, no finding:** `f16` storage-only
is airtight — every arithmetic route (binary, unary, `++`, compound,
inside a `@CStruct` field update, inside `xs[i] +=`, inside `reduce`/
`map` over `f16[]`, inside an explicitly instantiated generic) is
S014, and the dumped ship C contains no `_Float16`/`__fp16` operation.
`half.rs` was checked against CPython's binary16 over **all 65,536 bit
patterns plus ~654,000 further cases** (midpoints ±1 ULP, randoms
spanning 2^±30) with **zero mismatches**, covering RNE, 65504 vs
65520→∞, min normal, min subnormal, ties-to-even both directions,
negatives, `±0`, `NaN`. The offsetof proof was shown **non-vacuous** by
corrupting the language's narrow size/align rule in an out-of-repo copy
and observing the test fail.

## Gate (§16.3, all met — orchestrator-verified)

1. Standing gate byte-exact, both tiers, 50 goldens.
2. `offsetof` proof green for `SubNarrowPacket`, a genuine padding case
   (`kind`@0, `delta`@2, `weight`@4, `serial`@8, `bias`@16, `count`@18,
   `scale`@20; size 24, align 8 — a `uint64_t` forces align 8 and a
   2-byte interior hole). Language matches the platform C compiler
   field-for-field; independently re-derived with a C probe.
3. `npx tsc -p tsconfig.json` exit 0, unchanged config.
4. Rejects `r33`–`r37` at pinned S-codes.
5. `a49` pins the f16 conversion battery: representable `1.5`, rounded
   `1.0009765625`, overflow `Infinity`, subnormal
   `0.00000005960464477539063`, `NaN`, `-0`.
6. `SubNarrowPacket` (uint8_t/int16_t/_Float16/uint64_t/int8_t/uint16_t/
   float) binds through bindgen and passes its offsetof proof — the
   blocker this phase existed to remove is demonstrably removed.
7. Benchmarks re-captured post-fix: **no ship-row regression** (tree
   1.37×, sort 1.80×, particles 3.06×, compute-bound 0.97–1.07×).

457 tests, 0 failures, zero warnings, no pre-existing golden modified.
Both CRITICALs re-verified by the orchestrator with an independent
probe (three consecutive runs, identical output — the UB is gone).

## Follow-ups (not scheduled)

- `f16` as a by-value boundary **parameter** is unexercised at runtime:
  `SubNarrowPacket` is declared but no C function consumes it, so the
  offsetof proof is its only evidence.
- bindgen has no `--target`; plain `char` fails loud rather than being
  mapped per target. Adding an explicit target option would allow it.
