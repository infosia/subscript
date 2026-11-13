# Raw registration — section 263

## Implementation

The Context registration methods are unsafe and state the required value lifetime.
RunConfig keeps its provider field private to the crate and exposes an unsafe builder.
Each caller states the lifetime reason at its unsafe block.
The provider transfer into a run or session cites the builder contract.
No C export signature, header, corpus entry, or golden changes.

The source check also found three functions that retain pointer values:
`bind_callback`, `register_callback`, and `raise_exception`.
These functions are unsafe under section 263 rule 1.

## Red evidence

Before the API changes, the source test failed with these 14 entries:

```
async_is_stale
set_fn_table
set_globals
set_trap_observer
set_print_observer
set_diagnostics_observer
root_add
shadow_push
bind_callback
raise_exception
async_complete_exception
register_callback
release_callback_registration
file_provider
```

This list contains all seven Context functions and the RunConfig field from the contract table.
The firing control passed; the source audit failed as required.
The test read 3,939,476 source bytes in 118.03 ms. The command took 0.87 s.

## Allowlist

| Source function | Reason |
|---|---|
| runtime/src/context/async_scheduler.rs::async_is_stale | Uses the frame address only as a lookup key during the call. |
| runtime/src/exception.rs::async_complete_exception | Uses the frame address only as a lookup key during the call. |
| runtime/src/registration.rs::release_callback_registration | Uses the registration address only as a lookup key during the call. |

Each allowlist row names its source file and function. A missing or duplicate match fails the check.
An empty reason fails the check. A new function outside the allowlist fails the check.
The controls supply an unlisted pointer parameter and a stale allowlist row; both checks fail as required.
The controls also cover generic bounds, observer types, address parameters, and unsafe or crate-private exclusions.

## Cost and checks

The audit reads Rust source text under runtime/src and codegen/src, including cfg test modules.
It performs no compiler invocation or target-program execution.
It tokenizes each file once and checks function parameter tokens.
Its cost is linear in the source bytes, plus directory traversal and path sorting.
The final audit read 3,956,835 bytes in 201.71 ms.
This source pass supplies the total API check; no target execution can replace it.

| Command or test | Result | Time |
|---|---|---|
| cargo build --workspace --all-targets | PASS | 31.20 s wall |
| cargo test --offline --locked -p subscript-runtime --test raw_registration -- --nocapture | PASS: 2 tests | 0.20 s tests; 0.73 s wall |
| cargo test --offline --locked -p subscript-runtime -p subscript-codegen --lib | PASS | 71.53 s wall |
| codegen library tests | PASS: 340 | 65.28 s tests |
| runtime library tests | PASS: 405; 1 ignored | 0.87 s tests |
| cargo fmt --check | PASS | 1.99 s wall |
| cargo clippy --offline --locked --workspace --all-targets | PASS with warnings | 0.38 s wall |

Clippy warnings in changed files:

- runtime/src/arrops.rs: five `too_many_arguments` warnings.
- runtime/src/assocops.rs: one `too_many_arguments` warning.
- runtime/src/worker.rs: one `vec_box` warning.
- runtime/src/context/tests.rs: four `doc_lazy_continuation` warnings.

These warnings refer to existing declarations or comments, outside the changed lines.
No new unsafe block lacks a SAFETY comment.

## Changed files

- `codegen/src/interpreter.rs`
- `codegen/src/interpreter/roots.rs`
- `codegen/src/jit/entry.rs`
- `codegen/src/lib.rs`
- `codegen/src/lower/slot_tests.rs`
- `codegen/src/reload.rs`
- `codegen/tests/support/files.rs`
- `runtime/src/arrops.rs`
- `runtime/src/assocops.rs`
- `runtime/src/context/async_all_tests.rs`
- `runtime/src/context/async_scheduler.rs`
- `runtime/src/context/lifecycle.rs`
- `runtime/src/context/memory.rs`
- `runtime/src/context/task_group_tests.rs`
- `runtime/src/context/tests.rs`
- `runtime/src/context/tests_0.rs`
- `runtime/src/context/tests_1.rs`
- `runtime/src/exception.rs`
- `runtime/src/exception/async_completion_tests.rs`
- `runtime/src/ffi.rs`
- `runtime/src/ffi/callbacks.rs`
- `runtime/src/ffi/files.rs`
- `runtime/src/ffi/memory.rs`
- `runtime/src/regexops.rs`
- `runtime/src/registration.rs`
- `runtime/src/worker.rs`
- `runtime/tests/async_budget.rs`
- `runtime/tests/counted_array_holders.rs`
- `runtime/tests/counted_collection.rs`
- `runtime/tests/counted_map.rs`
- `runtime/tests/counted_object.rs`
- `runtime/tests/host_operation.rs`
- `runtime/tests/zero_length_store.rs`
- `runtime/tests/raw_registration.rs`
- `specs/tracking/raw-registration.md`

## Source check scope

The record-type list is an approximation. The check knows FileProvider and the observer types.
It also detects raw pointers and NonNull, including safe public const, async, and extern functions.
CallbackBinding, SubStrView, WorkerMessageDescriptor, and CompletionEndpoint contain raw-pointer fields.
No safe public function takes these records today.
Macro function names include the macro name so each allowlist row identifies one declaration.
The pure Math macro declarations and six numeric exports ignore the Context pointer and do not retain it.
The extended check reported all eight declarations before the allowlist update; no C export retains a pointer.

| Additional source function | Reason |
|---|---|
| `runtime/src/ffi/conversions.rs::math_ffi_unary::sym` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/conversions.rs::math_ffi_binary::sym` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_fmod` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_math_clz32` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_math_imul` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_math_fround` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_math_f32_to_bits` | Ignores the Context pointer; the operation is pure. |
| `runtime/src/ffi/numbers.rs::subscript_rt_math_f32_from_bits` | Ignores the Context pointer; the operation is pure. |

## Follow-up checks

The controls cover safe public extern, const, and async functions, unsafe extern exclusions, and NonNull parameters.
The source check passed: 2 tests, 0.17 s test time, 0.24 s command time.
It read 3,957,738 bytes in 172.89 ms before the final local-comment corrections.

| Command | Result | Wall time |
|---|---|---|
| cargo build --workspace --all-targets | PASS | 21.02 s |
| cargo test -p subscript-runtime --lib | FAIL: 404 passed, 1 failed, 1 ignored | 3.10 s |
| cargo test -p subscript-codegen --lib | PASS: 340 tests | 48.92 s |
| cargo fmt --check | PASS | 2.02 s |
| cargo clippy --offline --locked --workspace --all-targets | PASS with existing warnings listed above | 5.63 s |

The runtime failure is `host_header::tests::generated_host_header_is_byte_identical`.
The generator copies the updated observer safety docs into the host header.
The task forbids changes to runtime/include/*.h, so the generated header remains unchanged.
The header needs regeneration before this test can pass.
No C export signature changed.

arrops.rs contains 2,623 lines at HEAD and 2,625 lines now.
The two shadow-push sites replace their existing comments without extra lines.
The two root-add sites each need one new safety comment.
The task forbids a file split in this round; the remaining increase is two lines.

Files changed in this follow-up:

- runtime/tests/raw_registration.rs
- runtime/src/arrops.rs
- runtime/src/context/memory.rs
- runtime/src/context/tests_0.rs
- runtime/src/registration.rs
- runtime/src/ffi.rs
- runtime/src/ffi/callbacks.rs
- runtime/src/ffi/memory.rs
- codegen/src/jit/entry.rs
- codegen/src/lib.rs
- specs/tracking/raw-registration.md

## Header and line-count checks

The host-header generator updated `runtime/include/subscript_runtime.h` in 1.30 s wall time.
The header diff against HEAD contains two added comment lines: one file, two insertions, no deletions.
After C comments are removed, the header matches HEAD byte for byte. No export signature changed.
Two existing array-operation comments now each use one line. `runtime/src/arrops.rs` contains 2,623 lines, equal to HEAD.

| Command or test | Result | Wall time | arrops.rs lines |
|---|---|---|---|
| cargo test --offline --locked -p subscript-runtime --lib host_header::tests::generated_host_header_is_byte_identical -- --exact | PASS: 1 test | 1.64 s | 2,623 |
| cargo test --offline --locked -p subscript-runtime --test raw_registration -- --nocapture | PASS: 2 tests | 1.88 s | 2,623 |
| cargo fmt --check | PASS | 2.39 s | 2,623 |

The source check read 3,957,723 bytes in 190.41 ms.
