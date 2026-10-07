# §174: A zero length store clears an array

## Corpus evidence

Contract pin: `69212b29`. TypeScript: stock 5.9.2. Node: v24.18.0.
Each TypeScript project extends the repository configuration and includes the language prelude.
Each CLI probe uses a temporary copy of its corpus source.
The pin binaries and the interpreter test use the contract source tree.

| Entry | TypeScript 5.9.2 | Pin dev JIT | Pin C AOT | Pin interpreter |
|---|---|---|---|---|
| `a344-zero-length-store` | accepts; exit 0 | S100; exit 1 | S100; exit 1 | Checker rejects with S100 |
| `t102-zero-length-store` | accepts; exit 0 | S100; exit 1 | S100; exit 1 | Checker rejects with S100 |
| `r395-array-length-store` | accepts; exit 0 | S100; exit 1 | S100; exit 1 | Checker rejects with S100 |

Every pin diagnostic selects `ArrayUnknownMember` and carries no divergence block.
Node prints the a344 golden: `number 3 1`, `string new 1`, `reference 7 1`, `handle 6 1`, and `parameter 0`.
The three execution tiers print the same golden after the change.
The new trap entry uses its full name because `t102-generator-parameter` already exists.

## Clear operation

The checker accepts a literal `0` store statement on a dynamic array.
The clear evaluates the receiver once and gives no expression value.
The HIR operation selects `BuiltinMethod::ArrayClear`.
One LIR call carries the element count action, the array invalidation, and the trap sites.
The verifier derives the count action from the receiver element type.
The verifier rejects a missing action, a wrong action, or a counted release without a Call trap.

The shared runtime operation uses opcode 3 for a clear.
It saves counted element bytes, clears the storage, and releases each removed element in index order.
A null description selects an uncounted clear.
The operation keeps the array holder count.
The interpreter releases its own task registry through the same static element type.
Both native tiers use the shared runtime release description.

## Rejection site

`ArrayLengthStore` carries S100 and `Diverges(ArrayLengthStore)`.
Nonzero, compound, variable, update, and expression stores reach this site.
A class field named `length` keeps its field semantics.
A FixedArray store keeps its existing site.
The message states: ``only `xs.length = 0` as a statement is accepted; use `splice` or `pop` to remove elements``.

The §154 table adds five TypeScript-accepted witnesses.
Its total test covers 1,682 witnesses and 477 variants.
The measured test costs 0.698 seconds for TypeScript, 0.712 seconds for the checker, and 1.708 seconds total.

### Required stdlib row

| Record | Required text |
|---|---|
| `stdlib.md` §9.12 | Only `xs.length = 0` as a statement clears a dynamic array. A nonzero length store can grow the array with holes. The language has no hole value. Use `splice` or `pop` to remove elements. Reject assignment expressions and compound length stores. |

## Count and trap evidence

The three-tier count test covers handle elements, handle-array elements, and nested-array elements.
Each shape includes a same-shape control that discards each pop result.
Each clear and control leaves zero retained tasks before Context teardown.
The six C builds and the two other tiers cost 3.58 seconds in the dedicated test.

Direct runtime tests check recursive releases, the unchanged holder count, an empty clear, and array reuse.
The release-order test reports the first element's exception and stops before the second release.
The clear and pop trap controls stop before the next print in each execution tier.
The LIR Call trap names the store; the uncaught-exception report retains the original throw position, 10:40.
A temporary receiver releases its holder after the clear.
A receiver function runs once, and its caller sees the empty array.

The LIR text golden adds only a344.
Every existing `.expected` file keeps its contents.
The document generator supplies the corpus index and language reference changes.

## Check evidence

| Check | Result |
|---|---|
| `cargo build --offline --locked --workspace --all-targets` | pass |
| `cargo test --offline --locked -p subscript-compiler` | pass |
| `cargo test --offline --locked -p subscript-codegen` | pass |
| `cargo test --offline --locked -p subscript-runtime` | pass |
| `cargo fmt --check` | pass |
| `cargo clippy --offline --locked --workspace --all-targets` | pass; no new warning |
| `tools/hygiene.sh` | pass |

Clippy reports the same 69 warning instances as the contract tree.
Every changed Rust file stays below 2,000 lines.

## Changed files

- `codegen/src/cemit/counted_array.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/counted.rs`
- `codegen/src/interpreter/counted_measurement_tests.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/verify_counted_operations.rs`
- `codegen/src/lir/verify_lifetime.rs`
- `codegen/src/lower/func/builtin.rs`
- `codegen/src/lower/func/counted_array.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/counted_operation_verifier.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/zero_length_store.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/hir.rs`
- `compiler/src/lir.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/operation_signatures.rs`
- `compiler/tests/zero_length_store.rs`
- `corpus/accept/a344-zero-length-store.expected`
- `corpus/accept/a344-zero-length-store.ts`
- `corpus/reject/r395-array-length-store.ts`
- `corpus/trap/t102-zero-length-store.expected`
- `corpus/trap/t102-zero-length-store.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `runtime/src/context/counted.rs`
- `runtime/src/ffi/async_frames.rs`
- `runtime/tests/zero_length_store.rs`
- `specs/tracking/s174-zero-length-store.md`
