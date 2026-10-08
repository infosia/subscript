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
- Check all completion floating-point leaves through stored-bit helpers; the measurement compares their exact finite printed values.
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

## Implementation: kind record and classes N, F, H, R

Rules 1–5 use the shared boundary kind record.

### Record and consumers

The new `subscript-boundary` crate owns the fundamental C kind records.
It has no dependencies, so bindgen, compiler, and codegen can use it without a cycle.
Each record carries the C spelling, language spelling, size, alignment, carrier, extension, and HFA leaf class.
The binder reads the C lookup in `lang_scalar`, which `map_use` calls.
The compiler projects semantic types through `types::boundary_kind`.
The layout builders read the record through `scalar_size_align`.
The JIT projects the carrier and width into Cranelift types.
The C emitter reads the C spelling and retains separate local storage types for half bits and booleans.
HFA classification reads the shared leaf class through the native type projection.

The JIT marks narrow signed arguments with `sext` and narrow unsigned and boolean arguments with `uext`.
Native return signatures use the declared width.
Half scalar crossings use bitcasts between `I16` storage and `F16` carriers.
Half aggregate leaves use `F16`.
The C emitter uses two `memcpy` helpers for half scalar and field crossings.
Struct returns retain byte copies. Completion results retain byte copies.
Scratch writeback copies half bits and recurses into nested value fields in both tiers.

### Target rules and source limits

Apple arm64 native tests execute this implementation.
Other targets do not execute on this host. Their rules below are document claims *(docs)*.

| Target | Aggregate return rule | Source |
|---|---|---|
| AAPCS64 | An HFA of 1–4 identical half, float, or double leaves returns each leaf in `v0` through `v3`. Other small composites use general registers; larger composites use the result pointer in `x8`. | [AAPCS64 §6.8.2 and §6.9](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst) *(docs)* |
| x86-64 SysV | Each eightbyte has its own class. SSE images use `XMM0` and `XMM1`; INTEGER images use `RAX` and `RDX`. MEMORY results use a hidden pointer. HFAs have no separate rule. | [AMD64 psABI, returning values](https://raw.githubusercontent.com/wiki/hjl-tools/x86-psabi/x86-64-psABI-1.0.pdf), [current psABI project](https://gitlab.com/x86-psABIs/x86-64-ABI) *(docs)* |
| Win64 | HFAs have no special case. C aggregates of 1, 2, 4, or 8 bytes return in `RAX`; other sizes use a hidden pointer. | [Microsoft x64 calling convention, return values](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention) *(docs)* |

Cranelift 0.125.4 supports `F16` register transfers, loads, stores, and bitcasts on AArch64 and x86-64 *(docs)*.
The implementation uses these transfers, not half arithmetic.
The AArch64 backend assigns `F16` to the float register class.
The x86-64 backend assigns half scalars to XMM registers under SysV.
Windows Fastcall requires `enable_llvm_abi_extensions` for `F16` arguments and returns *(docs)*.
The dev flags enable that setting. It follows LLVM's convention where MSVC defines no half scalar ABI *(docs)*.
MSVC still cannot compile the native `_Float16` fixture.
Sources: Cranelift 0.125.4, `src/isa/aarch64/inst/mod.rs::rc_for_type`, `src/isa/x64/abi.rs::compute_arg_locs`.
The package source pin is `4c22e15bade0a8590d8272f5be85426cb134601b`.
[Cranelift package source](https://docs.rs/crate/cranelift-codegen/0.125.4/source/),
[LLVM ABI extension setting](https://docs.wasmtime.dev/api/src/cranelift_codegen_meta/shared/settings.rs.html).

### Gate coverage

The gate set in “Implementation: corpus, gate sweep, and cost” replaces the earlier test lists and counts.
The final native gate has 549 checks per tier. The raw-bool gate has ten golden lines per tier.
The corpus contains 336 accept entries. The record has twelve scalar kinds.

### Changed files

| Files | Reason |
|---|---|
| `boundary/Cargo.toml`, `boundary/src/lib.rs`, `boundary/tests/kinds.rs` | Add the dependency-free record and its contract and C compiler tests. |
| `Cargo.toml`, `Cargo.lock` | Register and resolve the shared workspace crate. |
| `bindgen/Cargo.toml`, `bindgen/src/emit.rs` | Read the record during C scalar mapping. |
| `compiler/Cargo.toml`, `compiler/src/types.rs` | Share semantic kind mapping and storage layout facts. |
| `codegen/Cargo.toml`, `codegen/src/layout.rs` | Project the record into storage and native register types. |
| `codegen/src/lower/func.rs`, `codegen/src/lower/func/abi.rs`, `codegen/src/lower/func/boundary.rs` | Carry extension, half SIMD leaves, typed return images, and nested scratch writeback. |
| `codegen/src/lower/mod.rs` | Enable the Cranelift setting required for Win64 half transfers. |
| `codegen/src/cemit/emitter.rs`, `codegen/src/cemit/marshal.rs`, `codegen/src/cemit/call.rs` | Emit half bit-copy helpers and consume them at native crossings. |
| `codegen/tests/native-fixture/cases.rs`, `codegen/tests/abi_pressure.rs` | Extend the generated native archive, ABI sweep, and completion sweep. |
| `codegen/tests/interop.rs` | Replace the retired HFA refusal test with a native bit comparison. |
| `specs/tracking/s179-boundary-abi-measurement.md` | Record the implementation and its evidence. |

Generated fixture artifacts change through `cases.rs` only.

## Implementation: bool storage and layout assertions

Rules 6–8 give bool fields their C storage and compare every emitted value-class layout with the layout builder.
C storage uses the shared boundary record's `bool` type. SSA locals retain `int32_t`.
C assignments convert between these forms. A bool store produces zero or one.
The common mapping covers class fields, nested classes, arrays, and completion carriers.
Struct copies use the C type whose layout the assertions check.
Field loads and stores use typed C lvalues. JSON readers use the layout builder's field offsets.
`Promise.all` elements and completion buffers use the same storage type.
The dev JIT uses the record's one-byte `I8` representation.
Its bool constants use zero or one; comparisons and logical negation produce zero or one.
The native byte helper also checks the two stored bytes after field writes in both tiers.

The emitter adds `sizeof`, `_Alignof`, and every field's `offsetof` assertion for every emitted value class.
Each assertion reads `Layouts::build_lir`, which the dev JIT also uses.
The unit test builds a separate scratch layout with `i32` fields and emits the original bool declaration.
It keeps both layout records intact. The C compiler reports size, alignment, and field-offset errors.

The binder, checker, and LIR verifier now admit completion structs with bool leaves.
Acceptance tests retain the former bool-pair, padded-bool, bool-alias, and nested-bool shapes.
The completion sweep adds bool pairs, padded bool/i32/bool, and nested bool leaves.
These types use native, function, method, and closure producers, with immediate, held, and `Promise.all` readers.
The padded native producer writes fields over `0xa5` bytes.

The native storage test reads `false:37:false` from a padded C return and a pointer writeback in both tiers.
It also checks script value copies, nested fields, array elements, JSON, and canonical stored bool bytes.
The rule 7 assertions prove that the script-only two-bool class has size 2 and alignment 1.

### Negative controls

A scratch checkout restores the four-byte C bool mapping.
With assertions, the C build fails: `8 == 2`, `4 == 1`, and incorrect field offsets.
With assertions removed only in the scratch checkout, the same native test fails.
The C AOT return and writeback both print `true:37:true` instead of `false:37:false`.
The dev JIT passes the same test before each C AOT failure.

### Bool storage files

| File | Change |
|---|---|
| `bindgen/src/completion.rs` | Remove the bool-leaf admission refusal. |
| `compiler/src/check/mirror_provenance.rs` | Admit resolved bool fields, including aliases and nested leaves. |
| `codegen/src/lir/completion.rs` | Remove the bool-field verifier refusal. |
| `codegen/src/cemit/emitter.rs` | Use the storage record and emit total layout assertions. |
| `codegen/src/cemit.rs` | Add the standard bool header and the C-compiler layout unit test. |
| `codegen/tests/native-fixture/cases.rs` | Add native bool bytes, padded return/writeback, and completion producers. |
| `codegen/tests/abi_pressure.rs` | Add the native storage test and extend the completion sweep. |
| `codegen/tests/lir_completion.rs` | Replace the bool rejection gate with acceptance at both depths. |
| `cli/tests/bind_completion.rs` | Replace the four bool rejection shapes with binder/checker acceptance. |
| `specs/tracking/s179-boundary-abi-measurement.md` | Record this implementation and its measurements. |

`codegen/src/cemit.rs` needs the standard bool header because script-only modules also use bool storage.
It also contains the unit test that accesses the emitter's type output.
`codegen/tests/lir_completion.rs` needs the changed gate because the verifier now admits bool fields.


### Gate coverage

The gate set in “Implementation: corpus, gate sweep, and cost” replaces the earlier test lists and counts.
The final native gate has 549 checks per tier. The raw-bool gate has ten golden lines per tier.
The corpus contains 336 accept entries. The bool storage and host layout section gives the current measured gate costs.

### C compile time against `261f9ce2`

The measurement uses Apple clang 21.0.0 on Apple arm64.
Each compiler invocation uses C11, `-O2`, `-fwrapv`, and `-ffp-contract=off`.
The span compiles the emitted `program.c` to an object. It excludes Rust builds, emission, linking, and execution.
Each workload has three discarded warm-ups and 21 timed samples per version.
The order alternates between the pin and the final implementation for each sample pair.
The table reports medians. It includes all workload files and the a22 performance-gate workload.

Each isolated median ratio is within the 1.05 criterion.

| Workload | Pin C compile, ms | Assertions C compile, ms | Ratio |
|---|---:|---:|---:|
| `a22` | 68.920 | 68.685 | 0.9966 |
| `callbacks` | 47.885 | 47.715 | 0.9964 |
| `collect` | 43.698 | 43.582 | 0.9973 |
| `fib-loop` | 32.558 | 32.608 | 1.0015 |
| `fib-recursive` | 32.254 | 32.284 | 1.0009 |
| `mandelbrot` | 33.890 | 34.091 | 1.0059 |
| `particles` | 41.699 | 41.672 | 0.9993 |
| `primes` | 33.548 | 33.599 | 1.0015 |
| `queen` | 32.984 | 33.352 | 1.0112 |
| `sort` | 45.707 | 45.893 | 1.0041 |
| `tree` | 41.512 | 41.711 | 1.0048 |
| `bound-call` | 35.823 | 36.231 | 1.0114 |
| `async-deep-chains` | 40.537 | 40.831 | 1.0072 |
| `async-held-handles` | 42.750 | 43.171 | 1.0099 |
| `async-settled-awaits` | 37.049 | 36.938 | 0.9970 |

These numbers measure C compilation only.

## Implementation: corpus, gate sweep, and cost

### Corpus pin and native fixture

`a348-boundary-values-have-c-form` has six golden output lines.
They are `1`, `1 1`, `1 1`, `1.5 2.5`, `false:37:false`, and `2`.
The C callees compare half scalar and HFA bits with `0x3e00` and `0x4100`.
The bool producer fills its padding with `0xa5` before it writes the three fields.
The C size helper reads the byte count from `Context.bytesOf<ScriptBools>`.
The script-only class has two bool fields. It has no boundary mirror declaration.
`subscript bind` generates `boundary-values.generated.d.ts` from the header.
The regeneration test compares the generated and committed mirrors byte for byte.
Stock TypeScript 5.9.2 accepts the entry with the prelude, with exit 0.

The contract CLI and its native JIT probe build from a scratch export of `f6888abb`.
The stock CLI emits the C program. The platform C compiler links its generated entry and the synthetic host archive.
The stock CLI `run` accepts neither ambient mirrors nor native libraries.
A temporary CLI-crate example supplies those inputs to the unchanged pin JIT API.
The same native library serves the pin and the implementation.

| Tier at `f6888abb` | Measured result |
|---|---|
| Dev JIT | Lowering refuses the f32 HFA return: `foreign homogeneous floating-point aggregate return is unsupported`. No stdout. |
| C AOT | Exit 0. Lines: `1`, `0 0`, `0 0`, `1.5 2.5`, `true:37:true`, `8`. The golden differs. |
| Interpreter | Excluded: calls the synthetic native interop library. |

### Gate set and removal controls

`boundary_gate_in_both_tiers` has 530 checks in one async script module per tier.
Both tiers compare each output line with the independent C callee result or an explicit golden value.
The fixture build compiles each C source once into one archive.
Boundary gate libraries link that archive instead of recompiling the fixture C sources.
Corpus host drivers still compile against each generated program header. Their export-dependent functions cannot use the archive.

| Required path | Gate witnesses |
|---|---|
| §178 rule 13 allocator | 459 argument checks: general prefixes 0–8 for general kinds; general prefixes 0, 7, 8 and SIMD prefixes 0–8 for SIMD kinds. Narrow i8/i16 and half scalars/HFAs are included. |
| Unsigned caller extension | Nine checks narrow a runtime u32 result, 70000, to u8, u16, and bool at general prefixes 0, 7, 8. |
| Scalar and HFA returns | Four checks: signed i8, half scalar, half HFA2, and f32 HFA2. The native checker reads the returned bits. |
| Fields and pointer writebacks | Each class N, F, H, B, R has a padded field struct. General prefixes 0, 7, 8 test input and pointer output: 30 checks. |
| Small and indirect aggregate returns | Each class has one small padded return and one return larger than 16 bytes: ten checks. The indirect result checks all marker fields and the measured leaf. |
| Bool storage and script layout | Dirty bool/i32/bool return, dirty pointer writeback, and the C helper's script-only size: three checks. |
| Native completion results | Each class has producers whose endpoints start at x1, x7, or on the stack. Immediate, held, and `Promise.all` readers contribute 15 checks. |

A return has a fixed ABI location. Argument prefixes do not move a return into another register or onto the stack.
The gate tests direct return registers and indirect memory returns where the ABI defines each form.
The HFA allocator covers the last SIMD register and stack spill for every one-to-four-leaf width.
Padded field inputs and pointer outputs cover general-register and stack positions.

The gate removes the duplicate 20-case composite module, seven-integer completion module, and standalone f32 HFA return test.
It removes 16 scalar/HFA return checks and six duplicate field/writeback checks from the wider argument sweep.
It replaces the 305-check completion Cartesian product with 15 native checks across the five classes and three endpoint positions.
These removed cases add no detection of a removed §179 rule beyond the retained witnesses.
The allocator positions and distinct boundary paths remain mandatory coverage.

The wider cases remain in `codegen/examples/abi_wide/sweep.rs`, without ignored tests.
From the workspace root, build the runtime and run the example:

```sh
cargo build --offline -p subscript-runtime
SUBSCRIPT_RUNTIME_STATICLIB="$PWD/target/debug/libsubscript_runtime.a" cargo run --offline -p subscript-codegen --example abi_wide
```
The example includes the original completion producers, readers, bool copies, arrays, nesting, and JSON checks.

Each removal control changes one rule on a separate scratch copy.
The final gate runs both tiers before it asserts their results.
Wrong-check counts exclude refused modules; a refused module is a separate failure.

| Reverted rule | JIT wrong checks | JIT refused modules | C AOT wrong checks | C AOT refused modules |
|---|---:|---:|---:|---:|
| 2: signed caller extension only (N) | 16 | 0 | 0 | 0 |
| 2: unsigned caller extension only (N) | 4 | 0 | 0 | 0 |
| 3: half carriers and bit transfers (F) | 29 | 0 | 159 | 0 |
| 4: half HFA classification (H) | 109 | 0 | 0 | 0 |
| 5: HFA return support (R) | 0 | 1 | 0 | 0 |
| 6: one-byte C bool storage (B) | 0 | 0 | 0 | 1 |

The bool control keeps the layout assertions. The C compiler reports layout assertion failures and refuses the module.
The HFA return control refuses the combined JIT module before execution. The C AOT module still passes.
The half control restores integer carriers in scalar native signatures and numeric casts in the C half helpers.
The classification control excludes half leaves from HFA recognition.
Each caller-extension control removes only the extension that its row names.

### Earlier gate cost

The bool storage and host layout cost table below replaces these measurements for the current tree.

Each test executable runs alone. The wall span includes process start, native execution, and C AOT compilation. It excludes Rust builds.

| Gate test | Seconds | Required work |
|---|---:|---|
| `boundary_gate_in_both_tiers` | 4.002913 | 459 allocator positions and 71 boundary witnesses share one module per tier. One C AOT compile tests all five classes and their completion endpoints. |
| `every_kind_has_the_contract_facts` | 0.006539 | Independent contract literals for every kind record field. |
| `storage_matches_the_native_c_compiler` | 0.024070 | One native C compile checks all twelve kind sizes and alignments. |
| `value_class_assertions_reject_an_independent_c_layout` | 0.862636 | Control and independent scratch layouts test the total C layout assertions. |
| `completion_bool_struct_shapes_are_admitted` | 0.076842 | Four binder shapes and both checker inputs admit bool leaves. |
| `binder_results_and_checker_results_agree` | 0.392640 | Retain the complete binder/checker acceptance and rejection matrix. |
| `verifier_accepts_completion_struct_bool_fields_at_every_depth` | 0.012880 | Direct and nested bool fields pass LIR admission. |
| `boundary_values_mirror_is_byte_identical_to_regeneration` | 0.026119 | Regenerate the new mirror from its C header. |

The boundary gate costs more than one second because the required allocator sweep compiles 459 native call positions in both tiers.
The completion witnesses also require real C result bytes and three distinct script readers.
The gate performs no duplicate native fixture C compilation.

### Release cost against `261f9ce2`

The binaries run alone, after the verification commands end.
Each subject has three timed samples. The ratio divides the current minimum by the pin minimum.
Execution subjects discard three warm-up calls. C compilation has no discarded sample.
The C compile span compiles the emitted `program.c` to an object with C11, `-O2`, `-fwrapv`, and `-ffp-contract=off`.
The ten standard workloads and `a22` use the existing AOT timing entry and `jit_bench`.
The async workloads use the existing async timing entry, which includes Context lifetime and checkpoints to quiescence.
The bound-call native timer keeps its warm-up floor and uses three timed samples instead of fifteen.
Its workload owns its timer. Its checksum depends on the number of warm-up iterations.
The other workload checksums agree between the pin and the current implementation.
All sample triples below use milliseconds, in execution order.

| Workload | Pin C compile samples | Current C compile samples | Best ratio |
|---|---|---|---:|
| `a22` | 74.238375, 77.923584, 76.116208 | 91.787583, 91.132709, 96.141500 | 1.227569 |
| `bound-call` | 46.247750, 45.461667, 48.265083 | 42.826458, 51.061875, 46.222083 | 0.942034 |
| `callbacks` | 72.154459, 60.247500, 63.106709 | 70.659292, 62.351292, 62.048500 | 1.029893 |
| `collect` | 60.627042, 64.147083, 55.937084 | 52.625417, 60.631583, 65.088334 | 0.940797 |
| `fib-loop` | 46.738833, 46.733750, 47.390500 | 46.830542, 47.381583, 47.476833 | 1.002071 |
| `fib-recursive` | 44.066458, 47.794250, 46.671792 | 43.583417, 47.095375, 47.146666 | 0.989038 |
| `mandelbrot` | 45.450459, 47.993916, 45.587792 | 46.275583, 47.883791, 45.127167 | 0.992887 |
| `particles` | 55.854792, 55.593208, 59.629208 | 63.058041, 66.450750, 57.683166 | 1.037594 |
| `primes` | 54.016750, 57.771208, 53.999833 | 38.447833, 36.775708, 37.391750 | 0.681034 |
| `queen` | 45.945375, 41.512959, 41.421125 | 39.129500, 37.447000, 35.863042 | 0.865815 |
| `sort` | 50.637333, 49.399333, 50.257167 | 49.426500, 48.157667, 48.637958 | 0.974865 |
| `tree` | 40.929875, 40.518416, 39.966834 | 41.389500, 40.028834, 40.726666 | 1.001551 |
| `async-deep-chains` | 41.209000, 39.273333, 39.731917 | 39.790250, 38.933000, 38.910208 | 0.990754 |
| `async-held-handles` | 42.581375, 41.024459, 41.093875 | 42.657750, 42.280458, 41.188958 | 1.004010 |
| `async-settled-awaits` | 37.205416, 36.540041, 36.259291 | 37.549375, 36.947541, 37.148000 | 1.018981 |

| Workload | Pin C AOT samples | Current C AOT samples | Best ratio |
|---|---|---|---:|
| `a22` | 6.975000, 7.065000, 7.003000 | 7.495000, 7.503000, 14.120000 | 1.074552 |
| `bound-call` | 0.017250, 0.017500, 0.018167 | 0.007458, 0.007417, 0.007459 | 0.429971 |
| `callbacks` | 53.470000, 54.047000, 52.143000 | 53.736000, 54.197000, 55.345000 | 1.030551 |
| `collect` | 53.864000, 53.182000, 52.934000 | 53.118000, 47.429000, 56.067000 | 0.896003 |
| `fib-loop` | 45.482000, 45.569000, 45.872000 | 43.124000, 45.513000, 45.536000 | 0.948155 |
| `fib-recursive` | 5.271000, 5.055000, 4.952000 | 4.926000, 4.925000, 4.926000 | 0.994548 |
| `mandelbrot` | 174.864000, 172.582000, 172.786000 | 171.066000, 172.247000, 172.163000 | 0.991216 |
| `particles` | 116.050000, 117.231000, 120.640000 | 198.546000, 194.752000, 154.115000 | 1.328005 |
| `primes` | 28.888000, 29.536000, 29.598000 | 29.300000, 31.698000, 28.808000 | 0.997231 |
| `queen` | 36.615000, 35.623000, 35.352000 | 33.993000, 34.106000, 34.104000 | 0.961558 |
| `sort` | 23.854000, 23.865000, 23.878000 | 23.870000, 23.938000, 23.891000 | 1.000671 |
| `tree` | 141.568000, 141.735000, 140.673000 | 140.128000, 141.184000, 140.874000 | 0.996126 |
| `async-deep-chains` | 12.411000, 12.318000, 12.317000 | 12.203000, 12.214000, 12.212000 | 0.990744 |
| `async-held-handles` | 4.795000, 4.754000, 4.765000 | 5.083000, 5.065000, 5.072000 | 1.065419 |
| `async-settled-awaits` | 17.569000, 17.483000, 17.520000 | 17.351000, 17.370000, 17.331000 | 0.991306 |

| Workload | Pin dev JIT samples | Current dev JIT samples | Best ratio |
|---|---|---|---:|
| `a22` | 121.155167, 116.184375, 122.829667 | 118.410208, 123.712250, 122.140916 | 1.019158 |
| `callbacks` | 428.396417, 419.988250, 422.360500 | 414.203000, 403.918583, 410.081625 | 0.961738 |
| `collect` | 176.275208, 175.139000, 180.232708 | 183.256708, 179.107791, 170.692750 | 0.974613 |
| `fib-loop` | 99.728583, 99.879709, 99.522334 | 98.579625, 97.849625, 97.831125 | 0.983007 |
| `fib-recursive` | 11.499166, 10.897375, 10.771334 | 10.901000, 11.105084, 11.556542 | 1.012038 |
| `mandelbrot` | 178.842834, 177.931042, 179.715750 | 178.120625, 177.901166, 178.639500 | 0.999832 |
| `particles` | 861.997167, 880.304209, 1063.924291 | 924.214667, 861.336917, 931.765708 | 0.999234 |
| `primes` | 43.563958, 43.541959, 44.033958 | 43.516750, 43.530625, 44.583167 | 0.999421 |
| `queen` | 48.138375, 47.351833, 48.130375 | 49.233416, 47.768833, 48.454917 | 1.008806 |
| `sort` | 45.591750, 45.676750, 45.782208 | 45.646708, 46.604000, 46.563417 | 1.001205 |
| `tree` | 548.105834, 546.968292, 568.070500 | 544.504959, 547.994792, 544.447541 | 0.995391 |

The three async workloads and the native self-timed bound-call workload have C AOT execution measurements only.

Acceptance 4 fails in this sample set. These ratios exceed 1.05:

- `a22`, compile: 1.227569.
- `a22`, aot: 1.074552.
- `particles`, aot: 1.328005.
- `async-held-handles`, aot: 1.065419.

The measurements do not establish the cause of these costs. No replacement sample set changes the recorded result.

### Goldens and verification

`cargo run --release -p subscript-compiler --bin generate-api-reference` regenerates the documentation.
Only `generated-docs/corpus-index.md` changes. It adds the a348 row.
`generated-docs/api-reference.md` and `generated-docs/language-reference.md` remain byte-identical.
The LIR text golden remains byte-identical. The new corpus entry has no async or generator function.
The new a348 expected file is the only added `.expected` file. No existing `.expected` file changes.

| Verification | Result |
|---|---|
| `cargo fmt --check` | Pass. |
| `cargo clippy --workspace --all-targets` | Pass, with warnings. |
| Boundary, bindgen, compiler, and native fixture crate tests | 1,251 pass; one existing test remains ignored. |
| Codegen crate tests | 863 pass; one existing test remains ignored. |
| CLI library, bind_callback_lifetime, bind_completion, commands, and watch tests | 53 pass. |
| Codegen corpus and interop targets | Pass in both tiers. The corpus compares all 336 entries. |
| Stock `tsc -p tsconfig.json`, TypeScript 5.9.2 | Pass. |
| New boundary mirror regeneration | Pass. |
| `git diff --check`, `tools/hygiene.sh` | Pass. |

### Final cumulative changed files

- `Cargo.lock`
- `Cargo.toml`
- `bindgen/Cargo.toml`
- `bindgen/src/completion.rs`
- `bindgen/src/emit.rs`
- `bindgen/tests/regen.rs`
- `boundary/Cargo.toml`
- `boundary/src/lib.rs`
- `boundary/tests/kinds.rs`
- `cli/tests/bind_completion.rs`
- `codegen/Cargo.toml`
- `codegen/examples/abi_wide.rs`
- `codegen/examples/abi_wide/sweep.rs`
- `codegen/src/cemit.rs`
- `codegen/src/cemit/call.rs`
- `codegen/src/cemit/emitter.rs`
- `codegen/src/cemit/marshal.rs`
- `codegen/src/cemit/worker.rs`
- `codegen/src/layout.rs`
- `codegen/src/lir/completion.rs`
- `codegen/src/lower/func.rs`
- `codegen/src/lower/func/abi.rs`
- `codegen/src/lower/func/boundary.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/mod.rs`
- `codegen/tests/abi_pressure.rs`
- `codegen/tests/interop.rs`
- `codegen/tests/lir_completion.rs`
- `codegen/tests/native-fixture/build.rs`
- `codegen/tests/native-fixture/cases.rs`
- `codegen/tests/native-fixture/lib.rs`
- `codegen/tests/support/native_fixture.rs`
- `compiler/Cargo.toml`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/types.rs`
- `corpus/accept/a348-boundary-values-have-c-form.expected`
- `corpus/accept/a348-boundary-values-have-c-form.ts`
- `corpus/interop/boundary-values.c`
- `corpus/interop/boundary-values.generated.d.ts`
- `corpus/interop/boundary-values.h`
- `generated-docs/corpus-index.md`
- `specs/tracking/s179-boundary-abi-measurement.md`

### Interpreter corpus cost

The release interpreter comparison uses the standing corpus selection, with `cost: benchmark` entries excluded.
Both versions run 269 entries and compare every output with its golden.
The pin has 65 declared native or semantic exclusions. The current tree has 66, because a348 needs native calls.
The temporary release harness uses that same benchmark exclusion in both versions.

The corpus comparison below excludes a22; it does not measure the full interpreter selection that includes the benchmark.
Each of the following binaries runs alone, three times, without a discarded warm-up.
The wall span includes process start, corpus loading, checking, lowering, interpretation, and golden comparisons.
The minimum-to-minimum ratio is 0.994599. It passes 1.05 for this corpus selection.

| Version | Sample 1, seconds | Sample 2, seconds | Sample 3, seconds |
|---|---:|---:|---:|
| pin | 1.459923291 | 1.124241000 | 1.117709041 |
| current | 1.527225292 | 1.111672292 | 1.155689667 |


## Cost attribution

The four cost misses cover three programs: `a22` compile, `a22` AOT, `particles` AOT, and `async-held-handles` AOT.
The reference remains `261f9ce2`. The tree contains the boundary ABI implementation.
The acceptance 4 sample set and its failed result above remain unchanged.

### C differences by rule

Both release CLIs emit each program from the same source copy in the temporary directory.
The comparison covers `program.c`, `program.h`, `program.alloc.h`, and `entry.c`.

| Rule or change | `a22` | `particles` | `async-held-handles` |
|---|---|---|---|
| Rule 1: shared kind record | No emitted difference. | No emitted difference. | No emitted difference. |
| Rule 3: half transport | No half helper, signature, or transfer difference. | No half helper, signature, or transfer difference. | No half helper, signature, or transfer difference. |
| Rule 6: bool storage | No declaration, layout, load, store, or local-width difference. | No declaration, layout, load, store, or local-width difference. | No declaration, layout, load, store, or local-width difference. |
| Rule 7: assertions | Three added assertions for `SubC1`: size 64, alignment 4, and `d3` offset 0. | Four added assertions for `SubC1`: size 16, alignment 8, `d3` offset 0, and `d4` offset 8. | No assertions added. |
| Bool support include | One added `#include <stdbool.h>` in `program.c`; one in `program.h`. | The same two includes. | The same two includes. |

These are all differences. The allocation header and the generated entry are byte-identical for each program.
Every executable C statement is identical. `particles` contains no boolean field or boolean storage change.
The includes support rule 6 but do not change any declaration in these three programs.
Rules 2, 4, 5, and 8 produce no emitted difference in this set.

### Interleaved controls

`no-assert` removes only the rule 7 lines from a scratch copy of the tree C.
`no-stdbool` removes only the added include from the scratch C and its header.
The first control does not apply to `async-held-handles`, which has no assertion difference.

Apple clang 21.0.0 targets Apple arm64 with C11, `-O2`, `-fwrapv`, and `-ffp-contract=off`.
C compile samples measure a fresh compiler process that compiles `program.c` to an object, without a link.
All AOT binaries link the same release runtime archive.
The standard AOT entry times the exported `main` call for `a22` and `particles`.
The async entry includes Context creation, initialization, execution, checkpoints to quiescence, and Context release.
Each binary invocation discards three warm-up calls and records one timed call.
Every invocation reports a stable checksum. The checksums agree across all versions of each program.

Each subject has five interleaved rounds. No two binaries run together; no Rust build overlaps a sample.
Each round follows the table row order: pin 1, tree 1, applicable controls, pin 2, tree 2.
Pin 2 and tree 2 repeat the same C and binary within that round.
The sample columns give the rounds in execution order. All values use milliseconds.

#### `a22` C compile

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 66.836042 | 66.215750 | 65.479000 | 65.812084 | 65.631875 |
| tree-1 | 68.067750 | 66.141750 | 65.951084 | 65.574667 | 66.228375 |
| no-assert | 66.573500 | 65.484625 | 66.016125 | 65.861708 | 65.269042 |
| no-stdbool | 66.188333 | 66.022291 | 66.112292 | 66.313042 | 74.477750 |
| pin-2 | 65.609667 | 65.766709 | 68.152583 | 65.813000 | 67.160583 |
| tree-2 | 66.198000 | 66.269250 | 66.425416 | 65.965125 | 70.697750 |

#### `a22` C AOT

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 6.598 | 6.410 | 6.620 | 6.630 | 6.587 |
| tree-1 | 6.296 | 6.613 | 6.614 | 9.137 | 6.608 |
| no-assert | 6.267 | 6.587 | 6.589 | 6.621 | 6.579 |
| no-stdbool | 6.266 | 6.642 | 6.701 | 6.674 | 6.618 |
| pin-2 | 6.279 | 6.609 | 6.615 | 6.608 | 6.597 |
| tree-2 | 6.279 | 6.613 | 6.625 | 6.905 | 6.594 |

#### `particles` C compile

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 40.533917 | 39.993791 | 39.906875 | 39.714834 | 39.907167 |
| tree-1 | 39.679875 | 39.471167 | 39.773417 | 39.579500 | 40.243583 |
| no-assert | 39.981708 | 39.886291 | 40.881375 | 40.178083 | 41.053416 |
| no-stdbool | 39.610917 | 40.069625 | 41.168041 | 40.554000 | 41.748625 |
| pin-2 | 39.449125 | 39.896791 | 39.988291 | 39.552459 | 40.188625 |
| tree-2 | 39.888458 | 40.096542 | 39.487500 | 42.146791 | 40.049125 |

#### `particles` C AOT

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 88.033 | 87.116 | 86.717 | 87.584 | 92.916 |
| tree-1 | 87.873 | 87.487 | 86.608 | 88.389 | 98.362 |
| no-assert | 87.585 | 86.997 | 87.874 | 94.098 | 99.077 |
| no-stdbool | 87.586 | 86.701 | 86.902 | 93.196 | 93.843 |
| pin-2 | 88.076 | 87.254 | 87.018 | 92.829 | 89.427 |
| tree-2 | 87.505 | 87.504 | 87.614 | 92.864 | 90.294 |

#### `async-held-handles` C compile

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 37.491375 | 34.498541 | 34.796250 | 35.582833 | 35.534417 |
| tree-1 | 35.606334 | 34.624000 | 34.450542 | 35.607458 | 34.806792 |
| no-stdbool | 35.813792 | 34.800250 | 34.345584 | 34.604833 | 35.191000 |
| pin-2 | 35.463375 | 34.488292 | 34.345250 | 34.739375 | 34.783500 |
| tree-2 | 35.191916 | 34.417625 | 34.328834 | 34.950833 | 34.842833 |

#### `async-held-handles` C AOT

| Version | Sample 1 | Sample 2 | Sample 3 | Sample 4 | Sample 5 |
|---|---:|---:|---:|---:|---:|
| pin-1 | 4.215 | 4.204 | 4.375 | 4.350 | 4.380 |
| tree-1 | 4.200 | 4.225 | 4.342 | 4.345 | 4.349 |
| no-stdbool | 4.216 | 4.297 | 4.392 | 4.353 | 4.366 |
| pin-2 | 4.219 | 4.208 | 4.369 | 4.375 | 4.367 |
| tree-2 | 4.210 | 4.353 | 4.373 | 4.367 | 4.470 |

### Causes and prototype decision

The following ratios divide each tree series minimum by its corresponding pin series minimum.
The median ratio uses all ten tree samples and all ten pin samples for that subject.

| Original miss | Original ratio | Series 1 ratio | Series 2 ratio | Combined median ratio | Attribution result |
|---|---:|---:|---:|---:|---|
| `a22` compile | 1.227569 | 1.001461 | 1.005418 | 1.006088 | No miss in either interleaved series. |
| `a22` aot | 1.074552 | 0.982215 | 1.000000 | 1.001514 | No miss in either interleaved series. |
| `particles` aot | 1.328005 | 0.998743 | 1.005585 | 0.999260 | No miss in either interleaved series. |
| `async-held-handles` aot | 1.065419 | 0.999049 | 1.000475 | 0.997361 | No miss in either interleaved series. |

For `a22` compilation, neither assertion removal nor include removal shows a repeatable reduction that explains the original 22.8% excess.
The tree compile range is 65.574667–70.697750 ms; the pin range is 65.479000–68.152583 ms.
The assertion control range is 65.269042–66.573500 ms. The include control range is 66.022291–74.477750 ms.
The include control retains the assertions. Its largest sample exceeds every tree sample.

For `a22` execution, pin and tree minima overlap; tree 1 also contains a 9.137 ms sample.
For `particles`, later samples rise across pin, tree, and both controls in the same interleaved set.
The pin range is 86.717–92.916 ms. The tree range is 86.608–98.362 ms.
The assertion control reaches 99.077 ms, despite removal of every added assertion.
For `async-held-handles`, the pin range is 4.204–4.380 ms; the tree range is 4.200–4.470 ms.
These samples show time variation within unchanged versions. They do not establish the machine condition behind the original samples.

The complete optimized C objects are byte-identical across pin, tree, and all applicable controls for each program.
The optimized assembly also agrees after removal of the source filename directive.
The object SHA-256 values are:

| Program | SHA-256, shared by all applicable versions |
|---|---|
| `a22` | `42989519f5aee52065257104c57baeb29e02b75445a8dfa0db151d22175acc25` |
| `particles` | `9b3e44481fa7b30e0752e0fdb702aca28c90250a59d3163ae9bfa187065e60f1` |
| `async-held-handles` | `0c10dcf95ec49cbe95403903c90b4a20e45ca48bd715752251c7f71e272996a6` |

The emitted changes introduce no machine-code difference in these subjects.
The interleaved series establishes no implementation cost miss for any of the four subjects.
The original acceptance failure remains recorded; these attribution samples do not replace it.
No real cause requires a fix in this set. Therefore, no compiler prototype or prototype measurement applies.

## Implementation: bool storage and host layout

### Caller extension witnesses

Nine additional native checks narrow the runtime result 70000 to u8, u16, and bool.
General prefixes 0, 7, and 8 cover the first register, last register, and stack.
The native gate has 549 checks per tier: 459 allocator positions and 90 boundary witnesses.
Separate scratch controls remove only unsigned extension or only signed extension.
Unsigned removal fails four JIT checks. Signed removal fails sixteen JIT checks. Both C AOT controls pass.

### Raw bool bytes

`Layouts::bool_offsets` derives each bool byte offset from stored field offsets and array strides.
Both `Context.fromBytes` lowerings copy the input bytes, then store one for every nonzero bool byte.
The JIT loads each byte as I8, compares it with zero, and stores the comparison result.
C uses unsigned-char accesses before any typed bool read. Thus, the normalization never reads an invalid C bool.
The gate checks the reported probe, bytes 0 and 1, nested classes, fixed arrays, and arrays inside classes.
The reported probe prints `true false true 10`. Its stored bytes are `1,1` in both tiers.

The raw-byte path audit covers these sites:

| Site | Byte source and result |
|---|---|
| `Context.fromBytes`: JIT `context_bytes_intrinsic`; C `emit_context_bytes` | Accepts arbitrary u8 input. Both sites normalize every bool byte from layout facts. |
| `Context.bytesOf` and `Context.bytesInto` | Copy typed values into u8 arrays. They do not write bytes into bool storage. |
| Worker payload decode, `runtime/src/worker.rs` | Copies private records from typed script payload serialization. Bool storage already contains 0 or 1. |
| Completion cache reads, JIT coroutine and C suspend lowerings | Copy typed script results or native C results. Host completion producers can supply arbitrary bytes. Both readers normalize bool offsets, including aggregate-array elements. |
| Native struct returns and pointer writebacks, both boundary lowerings | Copy native C values. C bool stores supply canonical bytes; padding never becomes a bool field. |
| Value assignments, arrays, and suspended activation copies | Copy typed values whose bool stores already contain 0 or 1. |
| DataView and typed array byte access | Expose numeric scalar storage, with no raw bool-valued destination. |

`Context.fromBytes` and host completion values expose arbitrary bytes as values with bool storage.
A scratch control removes both normalization loops. The raw-bool gate fails in both tiers.
Without normalization, the JIT probe prints `true true false 10`; C AOT prints `true true true 20`.

### Host layout comparison and shared facts

The emitter compares host-header size and field offsets for each boundary struct that the tiers copy as bytes, including fixed-array fields.
The recursive derivation excludes converted descriptors, string views, callback fields, and pointers that need conversion.
`SGPUProbeTextureDescriptor` keeps its converted language layout.
An independent host declaration with i32 bool fields fails size and field-offset assertions.
A field-count difference between an independent layout and the emitted class produces an emitter error.

The checker and completion verifier read C spellings and storage facts from `subscript-boundary`.
Half boundary consumers read the record's leaf class. The semantic kind lookup uses a match.
The record contains twelve language scalar kinds. The unsupported `void*` entry is removed.
The wider example reports 821 checks per tier and zero failures after all comparisons pass.

### Contract pin

The a348 pin is `864eb5af`. Only its pin hash changes.
The CLI and JIT probe build from that pin. Their results match the recorded pin results.
C AOT prints `1`, `0 0`, `0 0`, `1.5 2.5`, `true:37:true`, and `8`.
The JIT refuses the f32 HFA return with no stdout.

### Earlier bool storage and host layout gate costs

Each debug test executable runs alone after all build, test, and wider-example processes end.
The wall time includes process start and C AOT work. It excludes Rust builds.

| Changed gate test | Seconds | Required work |
|---|---:|---|
| `boundary_gate_in_both_tiers` | 4.002913 | 530 native witnesses per tier, one module per tier, one fixture archive. |
| `raw_bool_bytes_are_canonical_in_both_tiers` | 0.525034 | Ten golden lines per tier cover dirty bytes, canonical controls, nesting, and value-class arrays. |
| `every_kind_has_the_contract_facts` | 0.006539 | Independent facts for twelve scalar records, including rejected C spellings and the removed pointer name. |
| `storage_matches_the_native_c_compiler` | 0.024070 | One C compile checks twelve scalar sizes and alignments. |
| `value_class_assertions_reject_an_independent_c_layout` | 0.862636 | Four C compiles check language and host layouts; an independent short layout checks zip completeness. |
| `completion_bool_struct_shapes_are_admitted` | 0.037882 | Four binder shapes and both checker inputs admit bool leaves. |
| `binder_results_and_checker_results_agree` | 0.238201 | Complete binder/checker acceptance and rejection matrix. |
| `verifier_accepts_completion_struct_bool_fields_at_every_depth` | 0.691321 | Direct and nested bool fields pass LIR admission. |
| `boundary_values_mirror_is_byte_identical_to_regeneration` | 0.016089 | Regenerate the boundary mirror from its C header. |

### Header-only boundary mirror resolution

The header-only probe declares `OnlyValue { bool a; bool b; }` with no foreign functions.
The script constructs the value and reads its first bool field.
Both tiers print `true`.

`collect_mirror_provenance` records the header when a mirror declares boundary classes and supplies a header directive.
Each HIR boundary class carries `boundary_header`; the LIR lowering copies that identity.
The C emitter reads each compared class's header identity and emits its include before the host layout assertions.
The emitter removes duplicate includes across classes and foreign functions.
The lowering derives `copies_boundary_bytes` once for each class. Both tiers and the host comparison read that fact.
The derivation excludes converted fields and recursive value cycles. Opaque handles and intrusive header links retain their byte layout.
A class without a host header has no host comparison. An empty identity in the comparison set fails LIR verification.

`header_only_boundary_value_runs_in_both_tiers` checks the HIR identity, LIR identity, text output, and both tier outputs.
Its header contains only the bool struct; the test builds no native archive.
`verifier_rejects_compared_boundary_class_with_empty_header_identity` constructs a compared boundary declaration with an empty identity from an independent script value-class layout.
Its single finding reads `boundary class <id> has no header identity for host layout comparison`.
The verifier test performs no native compilation.

The LIR printer appends `header="..."` when a class carries a header identity.
The corpus text golden changes at 446 class lines, with no other changes.
The added identities name `interop.h` and `host-completion.h`.
`coroutine_and_measurement_lir_text_matches_goldens` regenerates the record through `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`.
The same test passes after the generated record replaces the golden.

### Earlier header identity gate costs

Each debug test executable runs alone after all other build and test processes end.
The wall time includes process start and C AOT work. It excludes Rust builds.

| New gate test | Seconds | Required work |
|---|---:|---|
| `header_only_boundary_value_runs_in_both_tiers` | 0.543332 | One module per tier and one C compile; HIR, LIR, and text identity checks; no native archive build. |

### Earlier header identity verification

| Check | Result |
|---|---|
| `cargo fmt --check` | Pass. |
| `cargo clippy --workspace --all-targets` | Pass with existing workspace warnings. |
| `cargo build --offline --locked --workspace --all-targets` | Pass; no warning line. |
| Boundary, bindgen, compiler, codegen, and native fixture crate tests | 2,117 pass; two existing tests remain ignored. |
| CLI library and every integration target except `gate` | 53 pass. |
| Corpus JIT/C AOT/golden comparison | All 336 entries pass. |
| Native gate sweep | 530 checks per tier pass. |
| Header-only probe and missing-header LIR verifier test | Pass. |
| Regenerated LIR text golden | Pass; only 446 class header suffixes change. |
| `git diff --check`, `tools/hygiene.sh` | Pass. |

### Completion bool bytes and fixed-array host layouts

`read_completion` and `emit_completion_read` normalize each bool offset after the cached result copy.
The layout supplies the offsets, as it does for `Context.fromBytes`.
The aggregate completion reader also normalizes array elements with the layout's stride and bool offsets.
C uses unsigned-char accesses before any typed bool read.
The runtime completion endpoint retains the supplied raw bytes; the reader applies the value type's layout.

The native gate adds eighteen completion checks and one fixed-array pointer check.
The scalar inputs are 0, 1, 2, and 255. The struct inputs are `{0, 1}` and `{2, 255}`.
Each input tests direct await, a stored Promise, and `Promise.all`.
An independent C pointer helper reads the struct's stored bytes. It expects `{0, 1}` or `{1, 1}`.
Before normalization, nine completion checks fail in each tier.
After scalar and struct normalization alone, three aggregate-array checks still fail in each tier.
The complete reader normalization passes every check in both tiers.

Each LIR class carries `copies_boundary_bytes`.
The lowering derives that fact once from the fields and their conversion requirements.
The derivation includes fixed arrays, opaque handles, and intrusive header links.
The C pointer marshaler, JIT pointer marshaler, array marshalers, nested copies, and host comparisons read that fact.
The scratch-scope decision reads the same fact.
A class without a header remains outside the host comparison set.
An empty header identity in that set fails verification.

The headerless test declares exactly `Pt { a: boolean; b: u8; }` without a constructor or header directive.
Both tiers emit that class and print `headerless`.
The header-only test remains the control that carries a host header.
The native `Mat` pointer test checks the array's fourth element and the bool writeback.
The C compilation test compares an exact four-element mirror and an independent stale three-element mirror.
The stale mirror fails both the host size assertion and the host field-offset assertion.
The by-value C array-field defect remains outside this change (§179.3).

The semantic scalar projection test is removed because it repeats the mapping that it tests.
The boundary record tests retain their independent platform C compiler size and alignment check.

The contract CLI and native JIT probe build from a scratch export of `864eb5af`.
The C AOT output remains `1`, `0 0`, `0 0`, `1.5 2.5`, `true:37:true`, and `8`.
The JIT still refuses the f32 HFA return with no stdout.
Only the a348 pin hash changes in its corpus source.

### Completion and host layout gate costs

Each debug executable runs alone after all other task builds and tests end.
The wall time includes process start and native C work. It excludes Rust builds and the shared fixture archive build.
The new completion checks and pointer check add no module or C compiler start to the native sweep.
This table replaces the earlier measurements for these tests.

| Gate test | Seconds | Required work |
|---|---:|---|
| `boundary_gate_in_both_tiers` | 4.283847 | 549 native checks per tier, one module per tier, and one shared fixture archive. |
| `raw_bool_bytes_are_canonical_in_both_tiers` | 0.567442 | Raw-byte, canonical, nested, and fixed-array storage checks in both tiers. |
| `header_only_boundary_value_runs_in_both_tiers` | 0.452539 | One module per tier and one C compile; header identity and output control. |
| `headerless_boundary_value_runs_in_both_tiers` | 0.463976 | One module per tier and one C compile; language assertions remain, host assertions are absent. |
| `fixed_array_boundary_pointer_compares_host_layout` | 0.097250 | Two C compiles: one exact mirror and one independent stale mirror. |
| `verifier_rejects_compared_boundary_class_with_empty_header_identity` | 0.006413 | One lowering and one verifier pass; no native compilation. |

### Completion and host layout verification

| Check | Result |
|---|---|
| `cargo fmt --check` | Pass. |
| `cargo clippy --workspace --all-targets` | Pass with workspace warnings. |
| `cargo build --offline --locked --workspace --all-targets` | Pass; no warning line. |
| Boundary, bindgen, compiler, codegen, and native fixture tests | 2,118 pass; two existing tests remain ignored. |
| CLI library and every integration target except `gate` | 53 pass. |
| Corpus JIT/C AOT/golden comparison | All 336 entries pass; zero skips. |
| Native gate sweep | 549 checks per tier pass. |
| Headerless and header-only mirrors | Both tiers pass. |
| Exact and stale fixed-array mirrors | Exact mirror compiles; stale mirror fails host size and field-offset assertions. |
| CLI and native JIT probe built from `864eb5af` | Both pin results remain unchanged. |
| Stock TypeScript 5.9.2 | Pass. |
| `git diff --check`, `tools/hygiene.sh` | Pass. |
