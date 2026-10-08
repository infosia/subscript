# §179 boundary ABI measurement

Pin: `261f9ce2`. Date: 2026-10-08. Host: Apple arm64.
Apple clang 21.0.0 compiles the C callees at `-O2`.
The fixture build selects a capable clang through the existing resolver.
The test runs the dev JIT and C AOT against independently compiled C callees.

This note records a partial measurement of the whole boundary.
The measured set contains 636 script checks and 1,272 tier outcomes.
The baseline gives 186 wrong values and 40 refused checks.
The four original defect classes account for every wrong value in this set.
A fifth class records the documented refusal of float and double HFA returns.
The baseline also gives three C AOT layout disagreements.

## A. Sweep

The primary sweep contains 468 argument and return checks, plus 86 completion checks.
The additional sweep contains 82 checks for pointer writeback, absorbed fields, handles, and floating-point return bits.
The C callees return failure masks. Zero means that every argument matches.
The primary scalar mask uses bits 1, 2, 4, and 8 for the value, general prefixes, SIMD prefixes, and tail.
Aggregate masks use field bits, then 256, 512, and 1024 for general prefixes, SIMD prefixes, and tail.
The C callees compare scalar and field bytes with `memcmp`.
C writers fill padding with `0xa5` before they write the fields.
The script uses C pointer helpers to read stored floating-point bits.
The additional sweep checks every floating-point aggregate return leaf through those helpers.
The primary sweep also compares the exact printed finite values.

| Kind | Argument positions | Return and fields | Completion |
|---|---|---|---|
| Signed and unsigned integers, 8/16/32/64 bits | General prefixes 0, 7, 8, 9 | Scalar return; padded field input, return, and pointer writeback | Each width and sign |
| `bool` | General prefixes 0, 7, 8, 9 | Scalar return; padded field; two bools; bool/i32/bool; pointer writeback | Scalar only |
| `_Float16`, `float`, `double` | General and SIMD prefixes 0, 7, 8, 9 | Scalar return bits; padded field; HFA with 1–4 leaves; pointer writeback | Scalars, fields, and each HFA |
| C enum and scalar alias | General prefixes 0, 7, 8, 9 | Scalar return; padded field input, return, and pointer writeback | Scalar and field |
| Nested boundary struct | General prefixes 0, 7, 8, 9 | Input and return fields | Each field |
| Opaque handle | General prefixes 0, 7, 8, 9 | Identity return and struct field input | Forbidden by §178 rule 6 |
| String view | General prefixes 0, 7, 8, 9 | Pointer field input and C writeback | Forbidden |
| Pointer/count descriptor | General prefixes 0, 7, 8, 9 | Embedded count/pointer field input | Forbidden |
| Callback | General prefixes 0, 7, 8, 9 | Callback-info field and synchronous delivery through two userdata slots | Forbidden |
| Alignment override | No host crossing under §62 rule 8 | Script-only size, alignment, and offsets | No boundary type |

A prefix of nine general arguments places an argument on the stack before the measured argument.
A SIMD prefix of nine also places a floating-point argument on the stack.
The general and SIMD prefix counts form a Cartesian product for floating-point scalars and HFAs.
Every argument function also checks a trailing integer.
The callback checks the message and the primary userdata object.

The scalar probes use these values:
`-31`, `231`, `-31001`, `61001`, `-31000001`, `3100000001`, `-1234567890123`, and `12345678901234`.
The bool value is false. The enum value is 3. The alias value is `3100000001`.
The half value is 1.5 (`0x3e00`). The float value is -31.5 (`0xc1fc0000`).
The double value is 37.5 (`0x4042c00000000000`). HFA leaves contain 1.5, 2.5, 3.5, and 4.5.
Pointer writeback probes start with zero fields and a true bool, so an omitted write cannot pass.

The binder accepts the standalone sweep header and emits 70,496 bytes of mirror text.
The executable uses a separate generated mirror with the same native types.
The binder rejects direct callback arguments and string-view, descriptor, and callback returns.
The binder also rejects string-field struct returns.
The script tests callback fields through the supported callback-info form.

### Failure classes

| Class | Failing cells | Tier | Wrong outcomes |
|---|---|---|---:|
| N: narrow caller extension | `i8` and `i16` arguments at general counts 0 and 7 | JIT | 4 |
| F: half transport | Scalar arguments and returns; C AOT field arguments; string-field struct writeback | JIT and C AOT | 101 |
| H: half HFA classification and return images | HFA arguments with 1–4 leaves; numeric and bit return reads | JIT | 76 |
| B: bool field storage | Padded struct return and writeback; bool/i32/bool return; three layout shapes | C AOT | 5 values; 3 layouts |
| R: float and double HFA return model | Returns with 1–4 leaves, in numeric and bit checks | JIT | 40 refused checks |

Unsigned narrow arguments and scalar bool arguments pass the measured values.
Their signatures still omit the caller extension fact. This sweep does not force dirty upper register bits for them.
Four half scalar argument cells pass accidentally when both register banks are exhausted.
Several half HFA cells also pass at exhausted register counts. These passes do not close the class.
The JIT reads different wrong register images across runs. The cell table records one baseline snapshot.

### Layout facts

The independent C probe compiles the actual emitted `SubC` definitions.
It compares their sizes, alignments, and field offsets with separately declared C structs.
The JIT side uses `value_class_layouts` from the checked module.
The probe compares all fields of 29 boundary structs and two script-only value classes.
All 31 JIT layouts match C. C AOT differs on the three rows below.
The bool/i32/bool struct has equal offsets and size, but its bool fields still occupy four bytes in C AOT.

| Struct | C size/align/offsets | JIT | C AOT |
|---|---|---|---|
| `{u8 head; bool value; u8 tail;}` | `3/1/0,1,2` | Equal | `12/4/0,4,8` |
| `{bool a; bool b;}` | `2/1/0,1` | Equal | `8/4/0,4` |
| Script-only `@ValueType` with two booleans | `2/1/0,1` | Equal | `8/4/0,4` |
| `{bool a; i32 b; bool c;}` | `12/4/0,4,8` | Equal | Equal numbers; bool width 4 |
| Script-only aligned f32 triple | `16/16/0,4,8` | Equal | Equal |

The bool prototype removes all three layout disagreements.

## B. Causes and closing rules

### N: caller extension

`codegen/src/lower/func/boundary.rs`, `marshal_foreign_argument`, obtains a storage representation from `Layouts::repr`.
`push_foreign_argument` creates an `AbiParam` without a signed or unsigned extension.
The native callee reads the wrong extended value for a negative narrow integer in a register.

Rule: derive each native parameter from its semantic type and target ABI, including the required caller extension.

The rule changes `marshal_foreign_argument` and `push_foreign_argument`.
The signature must carry signedness before those functions discard the language type.
Every native signature builder must use this mapping; ordinary script signatures retain their own convention.

A good-faith program reaches this class today:

```ts
export function main(): void {
  print(`${s179I8G0F0(-31, 97)}`);
}
```

The C callee expects zero and the JIT returns mask 1.
No existing golden fails. The narrow-field program `a47` does not test a narrow scalar register argument.

### F: half transport

`Layouts::repr` represents `f16` as `I16`. This storage choice does not describe its native register class.
`foreign_call` and `marshal_foreign_argument` use that representation as the native signature.
The JIT therefore selects general registers for a native half scalar.
C AOT stores half bits in `uint16_t`, but `marshal_foreign_value` passes that integer numerically to `_Float16`.
`marshal_boundary_struct` does the same conversion for a half field.
`emit_foreign_call` assigns a native half return numerically to the integer storage slot.
`emit_boundary_writeback` also converts a native half field numerically after a scratch-based pointer call.

Rule: separate half storage bits from its native carrier, and copy bits at every crossing in both directions.

The rule changes JIT argument and return signatures, plus their bit transfers.
It changes `cemit/marshal.rs` in `marshal_foreign_value`, `marshal_boundary_struct`, and `emit_boundary_writeback`.
It changes `cemit/call.rs` in `emit_foreign_call`.
The C emitter supplies two bit-copy helpers. The storage layout remains two bytes with alignment two.
The binder's `map_use` must preserve the half kind. A numeric cast cannot implement this transfer.

A good-faith program reaches this class today:

```ts
export function main(): void {
  print(`${s179HalfG0F0(1.5, 97)}`);
}
```

The C callee expects zero. The JIT returns mask 9 and C AOT returns mask 1.
A pointer-written string/half struct gives bits 1 in C AOT instead of 15872.
`a48` tests half arrays; it does not test scalar half register arguments.

### H: half HFAs

`boundary_leaf_components` gives a half leaf the `I16` class.
`abi.rs`, `is_pure_hfa_leaves`, recognizes only `F32` and `F64` leaves.
`plan_aggregate_arg` therefore sends a half HFA through integer images.
`plan_foreign_struct_return` likewise selects general return registers for it.

Rule: classify every native floating-point leaf before aggregate allocation, and carry the return register type in the plan.

The rule changes `boundary_leaf_components`, `is_pure_hfa_leaves`, and aggregate argument allocation.
It changes `plan_foreign_struct_return` and `finish_foreign_struct_return`.
The existing allocator already counts every CLIF float kind in NSRN.
The prototype uses `F16` leaves and typed SIMD return images on this host.
A shared boundary-kind record must feed the binder, signature builder, emitter, and layout builder.

A good-faith program reaches this class today:

```ts
export function main(): void {
  print(`${s179ArgS179HfaHalf2G0F0(new S179HfaHalf2(1.5, 2.5), 97)}`);
}
```

The C callee expects zero; the JIT reports wrong field and tail bits.
`a126` tests f32 HFAs. It does not cover half HFAs.

### B: bool storage

`codegen/src/cemit/emitter.rs`, `ctype`, maps `Type::Bool` to `int32_t`.
`emit_one_typedef` uses that type for boundary and script-only class fields.
The JIT layout uses one byte. The C header also uses one byte.
C AOT copies a native struct return into the larger internal type.
Nonzero native padding then becomes part of a script bool value.
The return copy size also exceeds the native object size.

Rule: map each field to its C storage type, including one-byte bool storage, before the emitter constructs any class layout.

The rule changes `ctype` and every field declaration that `emit_one_typedef` produces.
It covers script-only value classes, boundary classes, nested classes, arrays, and async result carriers.
The prototype maps bool to `uint8_t` throughout `ctype`; the native bool values remain zero or one.
A final implementation must also check completion admission and every copy size against the shared layout fact.
The measurement does not revise §178's exclusion of bool-field results.

A good-faith program reaches this class today:

```ts
export function main(): void {
  const value = s179ReturnS179BoolPad();
  print(`${value.a}:${value.b}:${value.c}`);
}
```

The expected result is `false:37:false`. C AOT reads `true:37:true` from the padded return.
A script-only `@ValueType` with boolean fields also reaches the layout defect without a host call.

### R: documented HFA return refusal

`plan_foreign_struct_return` rejects every pure f32 or f64 HFA return before it builds a register plan.
§12.3a documents this refusal. It is an existing supported-surface limitation, not an unrecorded silent failure.
The binder accepts the C function and the checker accepts the script call.

Rule: make the admitted native return form carry the complete target register image plan, or reject it at admission.

The prototype selects typed SIMD return registers in `plan_foreign_struct_return` on AAPCS64.
`finish_foreign_struct_return` stores each image at the appropriate leaf stride.
A complete rule must also state the other target ABI plans; this measurement tests Apple arm64 only.

A good-faith program reaches the refusal today:

```ts
export function main(): void {
  const value = s179ReturnS179HfaFloat2();
  print(`${value.v0}`);
}
```

The JIT reports `foreign homogeneous floating-point aggregate return is unsupported`.
C AOT returns the expected bits. No existing golden requires this return form.

## C. Prototypes and corpus

Each individual prototype starts from the pin. The combined prototype applies all four changes.
The return-only prototype isolates class R from half leaf classification.
Counts include refused checks. Layout disagreements form a separate count.

| Prototype | Primary wrong/refused before → after | Extra before → after | Existing corpus |
|---|---:|---:|---|
| Narrow caller extension | 193 → 189 | Unchanged by scope | 335/335, both tiers |
| Half bit transfer | 193 → 93 | 33 → 32 after the writeback helper | 335/335, both tiers |
| Half HFA and SIMD return plan | 193 → 107 | Not measured alone | 335/335, both tiers |
| Bool storage | 193 → 190; layouts 3 → 0 | Not measured alone | 335/335, both tiers |
| f32/f64 SIMD returns only | 193 → 173 | Not measured alone | 335/335, both tiers |
| Combined | 193 → 0 | 33 → 0 | 335/335, both tiers |

The corpus command is:

```sh
cargo test --offline -p subscript-codegen --test golden jit_ship_c_aot_and_golden_agree_byte_for_byte -- --nocapture
```

Every reported corpus run compares 335 entries with zero skips.
The runs cover the dev JIT, C AOT, and committed golden bytes.
The measurement does not run `tools/gate.sh` or a test that invokes it.
The corpus result here refers to the accept-golden sweep, not every integration test in the codegen crate.

## D. Cost and smaller sweep

The isolated primary binary takes 10.073 seconds with both tiers.
The isolated additional sweep takes 7.148 seconds.
The isolated smaller sweep takes 1.828 seconds.
These costs include native execution and C AOT compilation, but exclude the Rust test build.
The initial warm Rust test build takes 3.18 seconds, including the fixture archive build.
A clean dependency build cost remains unmeasured.
The original separate-module primary sweep takes 79.824 seconds for 446 script checks.
The combined-module form reduces repeated C compiler starts.

The smaller sweep uses five script checks and ten tier outcomes:

| Check | Baseline failing tier | Cause |
|---|---|---|
| Signed i8 argument, zero prefix | JIT | N |
| Half scalar argument, zero prefix | JIT and C AOT | F |
| Two-leaf half HFA argument, zero prefixes | JIT and C AOT | H and F |
| First bool of the padded native return | C AOT | B |
| First field of a two-leaf f32 HFA return | JIT refusal | R |

| Prototype omitted from the combined prototype | Wrong/refused cells | Binary time, seconds |
|---|---:|---:|
| None | 0 | 2.000 |
| Narrow extension | 1 | 1.565 |
| Half transfer | 3 | 1.683 |
| Bool storage | 1 | 1.832 |
| HFA argument and return plan | 2 | 1.522 |
| All prototypes | 7 | 1.888 |

This smaller sweep retains one witness for every measured cause.
It does not retain every argument position, field shape, or completion reader.
A gate can use this set for fast defect detection and run the wider set for ABI coverage.
The measurement does not install either set as a gate test.

## Limits

The measurement does not prove a total boundary contract.
The following work remains:

- Generate and execute the entire sweep from the binder's emitted mirror, including selected completion directives.
- Force dirty upper register bits for unsigned narrow and bool arguments.
- Compare independent layout numbers for absorbed records and opaque-handle field records.
- Check all completion floating-point leaves through stored-bit helpers; this round compares their exact finite printed values.
- Add completion `void` to the generated sweep; the existing corpus covers it.
- Extend the sweep to fixed-array fields and wire-mapped aliases from §52.
- Measure NaN payloads, signed zero, and subnormal values across each floating-point position.
- Run every codegen corpus suite, including trap and reload corpus suites, for each prototype.

The results above establish the four reported defect classes and one additional half writeback site.
They do not establish that no other class exists.

## File disposition

The following production files contain temporary prototype changes only. Their final bytes equal HEAD:

- `codegen/src/lower/func/boundary.rs`
- `codegen/src/lower/func/abi.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/marshal.rs`
- `codegen/src/cemit/call.rs`

The following tracked fixture files also return to HEAD:

- `codegen/tests/native-fixture/build.rs`
- `codegen/tests/native-fixture/lib.rs`

The following generated measurement files are removed:

- `codegen/tests/native-fixture/s179.c`
- `codegen/tests/native-fixture/s179.h`
- `codegen/tests/native-fixture/s179.rs`
- `codegen/tests/s179_measurement.rs`

The only retained file is `specs/tracking/s179-boundary-abi-measurement.md`.
No commit exists for this measurement.

Final repository checks:

```text
$ git status --short
?? specs/tracking/s179-boundary-abi-measurement.md
$ git diff --stat
(no output)
```

## Baseline failing cells

Each row gives a separately measured actual value and expected value.
`G` counts earlier general arguments. `F` counts earlier SIMD arguments.
The result columns show failure masks for argument cells, values for numeric cells, and unsigned integers for bit cells.
The refused return cells appear separately for each expected field.

| Class | Set | Tier | Cell | Actual | Expected |
|---|---|---|---|---|---|
| N | primary | JIT | `I8/arg/G0/F0` | `1` | `0` |
| N | primary | JIT | `I8/arg/G7/F0` | `1` | `0` |
| N | primary | JIT | `I16/arg/G0/F0` | `1` | `0` |
| N | primary | JIT | `I16/arg/G7/F0` | `1` | `0` |
| F | primary | JIT | `Half/arg/G0/F0` | `9` | `0` |
| F | primary | JIT | `Half/arg/G0/F7` | `9` | `0` |
| F | primary | JIT | `Half/arg/G0/F8` | `9` | `0` |
| F | primary | JIT | `Half/arg/G0/F9` | `9` | `0` |
| F | primary | JIT | `Half/arg/G7/F0` | `9` | `0` |
| F | primary | JIT | `Half/arg/G7/F7` | `9` | `0` |
| F | primary | JIT | `Half/arg/G7/F8` | `9` | `0` |
| F | primary | JIT | `Half/arg/G7/F9` | `9` | `0` |
| F | primary | JIT | `Half/arg/G8/F0` | `9` | `0` |
| F | primary | JIT | `Half/arg/G8/F7` | `9` | `0` |
| F | primary | JIT | `Half/arg/G9/F0` | `9` | `0` |
| F | primary | JIT | `Half/arg/G9/F7` | `9` | `0` |
| F | primary | JIT | `Half/return` | `-0.022094727` | `1.5` |
| F | primary | JIT | `Half/return-bits` | `42408` | `15872` |
| H | primary | JIT | `S179HfaHalf1/arg/G0/F0` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G0/F7` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G0/F8` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G0/F9` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G7/F0` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G7/F7` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G7/F8` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G7/F9` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G8/F0` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G8/F7` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G8/F8` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G8/F9` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G9/F0` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G9/F7` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G9/F8` | `1025` | `0` |
| H | primary | JIT | `S179HfaHalf1/arg/G9/F9` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf1/return-field/v0` | `-0.022094727` | `1.5` |
| H | primary | JIT | `S179HfaHalf2/arg/G0/F0` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G0/F7` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G0/F8` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G0/F9` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G7/F0` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G7/F7` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G7/F8` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G7/F9` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G8/F0` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G8/F7` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G8/F8` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G8/F9` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G9/F0` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G9/F7` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G9/F8` | `1027` | `0` |
| H | primary | JIT | `S179HfaHalf2/arg/G9/F9` | `1024` | `0` |
| H | primary | JIT | `S179HfaHalf2/return-field/v0` | `-0.022094727` | `1.5` |
| H | primary | JIT | `S179HfaHalf2/return-field/v1` | `8020` | `2.5` |
| H | primary | JIT | `S179HfaHalf3/arg/G0/F0` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G0/F7` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G0/F8` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G0/F9` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G7/F0` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G7/F7` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G7/F8` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G7/F9` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G8/F0` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G9/F0` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G9/F7` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/arg/G9/F8` | `1031` | `0` |
| H | primary | JIT | `S179HfaHalf3/return-field/v0` | `-0.022094727` | `1.5` |
| H | primary | JIT | `S179HfaHalf3/return-field/v1` | `8020` | `2.5` |
| H | primary | JIT | `S179HfaHalf3/return-field/v2` | `5.9604645e-8` | `3.5` |
| H | primary | JIT | `S179HfaHalf4/arg/G0/F0` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G0/F7` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G0/F8` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G0/F9` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G7/F0` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G7/F7` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G7/F8` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G7/F9` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G8/F0` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G9/F0` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G9/F7` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/arg/G9/F8` | `1039` | `0` |
| H | primary | JIT | `S179HfaHalf4/return-field/v0` | `-0.022094727` | `1.5` |
| H | primary | JIT | `S179HfaHalf4/return-field/v1` | `0.000018775463` | `2.5` |
| H | primary | JIT | `S179HfaHalf4/return-field/v2` | `5.9604645e-8` | `3.5` |
| H | primary | JIT | `S179HfaHalf4/return-field/v3` | `0` | `4.5` |
| F | primary | AOT | `Half/arg/G0/F0` | `1` | `0` |
| F | primary | AOT | `Half/arg/G0/F7` | `1` | `0` |
| F | primary | AOT | `Half/arg/G0/F8` | `1` | `0` |
| F | primary | AOT | `Half/arg/G0/F9` | `1` | `0` |
| F | primary | AOT | `Half/arg/G7/F0` | `1` | `0` |
| F | primary | AOT | `Half/arg/G7/F7` | `1` | `0` |
| F | primary | AOT | `Half/arg/G7/F8` | `1` | `0` |
| F | primary | AOT | `Half/arg/G7/F9` | `1` | `0` |
| F | primary | AOT | `Half/arg/G8/F0` | `1` | `0` |
| F | primary | AOT | `Half/arg/G8/F7` | `1` | `0` |
| F | primary | AOT | `Half/arg/G8/F8` | `1` | `0` |
| F | primary | AOT | `Half/arg/G8/F9` | `1` | `0` |
| F | primary | AOT | `Half/arg/G9/F0` | `1` | `0` |
| F | primary | AOT | `Half/arg/G9/F7` | `1` | `0` |
| F | primary | AOT | `Half/arg/G9/F8` | `1` | `0` |
| F | primary | AOT | `Half/arg/G9/F9` | `1` | `0` |
| F | primary | AOT | `Half/return` | `5.9604645e-8` | `1.5` |
| F | primary | AOT | `Half/return-bits` | `1` | `15872` |
| B | primary | AOT | `S179FieldBool/return-field/tail` | `0` | `23` |
| F | primary | AOT | `S179FieldHalf/arg/G0/F0` | `2` | `0` |
| F | primary | AOT | `S179FieldHalf/arg/G7/F0` | `2` | `0` |
| F | primary | AOT | `S179FieldHalf/arg/G8/F0` | `2` | `0` |
| F | primary | AOT | `S179FieldHalf/arg/G9/F0` | `2` | `0` |
| B | primary | AOT | `S179BoolPad/return-field/a` | `true` | `false` |
| B | primary | AOT | `S179BoolPad/return-field/c` | `true` | `false` |
| F | primary | AOT | `S179HfaHalf1/arg/G0/F0` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G0/F7` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G0/F8` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G0/F9` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G7/F0` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G7/F7` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G7/F8` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G7/F9` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G8/F0` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G8/F7` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G8/F8` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G8/F9` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G9/F0` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G9/F7` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G9/F8` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf1/arg/G9/F9` | `1` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G0/F0` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G0/F7` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G0/F8` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G0/F9` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G7/F0` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G7/F7` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G7/F8` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G7/F9` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G8/F0` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G8/F7` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G8/F8` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G8/F9` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G9/F0` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G9/F7` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G9/F8` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf2/arg/G9/F9` | `3` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G0/F0` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G0/F7` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G0/F8` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G0/F9` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G7/F0` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G7/F7` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G7/F8` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G7/F9` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G8/F0` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G8/F7` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G8/F8` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G8/F9` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G9/F0` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G9/F7` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G9/F8` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf3/arg/G9/F9` | `7` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G0/F0` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G0/F7` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G0/F8` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G0/F9` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G7/F0` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G7/F7` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G7/F8` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G7/F9` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G8/F0` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G8/F7` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G8/F8` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G8/F9` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G9/F0` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G9/F7` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G9/F8` | `15` | `0` |
| F | primary | AOT | `S179HfaHalf4/arg/G9/F9` | `15` | `0` |
| B | extra | AOT | `S179FieldBool/write/value` | `true` | `false` |
| B | extra | AOT | `S179FieldBool/write/tail` | `0` | `23` |
| H | extra | JIT | `S179HfaHalf1/return-bits/v0` | `41800` | `15872` |
| H | extra | JIT | `S179HfaHalf2/return-bits/v0` | `41760` | `15872` |
| H | extra | JIT | `S179HfaHalf2/return-bits/v1` | `28060` | `16640` |
| H | extra | JIT | `S179HfaHalf3/return-bits/v0` | `41720` | `15872` |
| H | extra | JIT | `S179HfaHalf3/return-bits/v1` | `0` | `16640` |
| H | extra | JIT | `S179HfaHalf3/return-bits/v2` | `1` | `17152` |
| H | extra | JIT | `S179HfaHalf4/return-bits/v0` | `41696` | `15872` |
| H | extra | JIT | `S179HfaHalf4/return-bits/v1` | `885` | `16640` |
| H | extra | JIT | `S179HfaHalf4/return-bits/v2` | `1` | `17152` |
| H | extra | JIT | `S179HfaHalf4/return-bits/v3` | `0` | `17536` |
| F | extra | AOT | `S179StringHalf/write-bits` | `1` | `15872` |
| R | primary | JIT | `S179HfaFloat1/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaFloat2/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaFloat2/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaFloat3/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaFloat3/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaFloat3/return-field/v2` | `codegen error` | `3.5` |
| R | primary | JIT | `S179HfaFloat4/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaFloat4/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaFloat4/return-field/v2` | `codegen error` | `3.5` |
| R | primary | JIT | `S179HfaFloat4/return-field/v3` | `codegen error` | `4.5` |
| R | primary | JIT | `S179HfaDouble1/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaDouble2/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaDouble2/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaDouble3/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaDouble3/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaDouble3/return-field/v2` | `codegen error` | `3.5` |
| R | primary | JIT | `S179HfaDouble4/return-field/v0` | `codegen error` | `1.5` |
| R | primary | JIT | `S179HfaDouble4/return-field/v1` | `codegen error` | `2.5` |
| R | primary | JIT | `S179HfaDouble4/return-field/v2` | `codegen error` | `3.5` |
| R | primary | JIT | `S179HfaDouble4/return-field/v3` | `codegen error` | `4.5` |
| R | extra | JIT | `S179HfaFloat1/return-bits/v0` | `codegen error` | `1069547520` |
| R | extra | JIT | `S179HfaFloat2/return-bits/v0` | `codegen error` | `1069547520` |
| R | extra | JIT | `S179HfaFloat2/return-bits/v1` | `codegen error` | `1075838976` |
| R | extra | JIT | `S179HfaFloat3/return-bits/v0` | `codegen error` | `1069547520` |
| R | extra | JIT | `S179HfaFloat3/return-bits/v1` | `codegen error` | `1075838976` |
| R | extra | JIT | `S179HfaFloat3/return-bits/v2` | `codegen error` | `1080033280` |
| R | extra | JIT | `S179HfaFloat4/return-bits/v0` | `codegen error` | `1069547520` |
| R | extra | JIT | `S179HfaFloat4/return-bits/v1` | `codegen error` | `1075838976` |
| R | extra | JIT | `S179HfaFloat4/return-bits/v2` | `codegen error` | `1080033280` |
| R | extra | JIT | `S179HfaFloat4/return-bits/v3` | `codegen error` | `1083179008` |
| R | extra | JIT | `S179HfaDouble1/return-bits/v0` | `codegen error` | `4609434218613702656` |
| R | extra | JIT | `S179HfaDouble2/return-bits/v0` | `codegen error` | `4609434218613702656` |
| R | extra | JIT | `S179HfaDouble2/return-bits/v1` | `codegen error` | `4612811918334230528` |
| R | extra | JIT | `S179HfaDouble3/return-bits/v0` | `codegen error` | `4609434218613702656` |
| R | extra | JIT | `S179HfaDouble3/return-bits/v1` | `codegen error` | `4612811918334230528` |
| R | extra | JIT | `S179HfaDouble3/return-bits/v2` | `codegen error` | `4615063718147915776` |
| R | extra | JIT | `S179HfaDouble4/return-bits/v0` | `codegen error` | `4609434218613702656` |
| R | extra | JIT | `S179HfaDouble4/return-bits/v1` | `codegen error` | `4612811918334230528` |
| R | extra | JIT | `S179HfaDouble4/return-bits/v2` | `codegen error` | `4615063718147915776` |
| R | extra | JIT | `S179HfaDouble4/return-bits/v3` | `codegen error` | `4616752568008179712` |
