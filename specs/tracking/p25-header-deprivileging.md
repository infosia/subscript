# P25 header deprivileging — exit evidence

Status: **COMPLETE**, measured 2026-07-28. Contract:
`specs/blocks/compiler.md` §23.8. Criteria 1–4 and 6–9 pass on the
current Unix host. Criterion 5 passes on both tiers on **both** hosts —
its Windows half was measured on `x86_64-pc-windows-msvc` (MSVC-only) once
the Windows ship tier moved to `cl` (`specs/tracking/windows-portability.md`).

## 1. A header other than the fixture binds and runs

Criterion: the engine facade binds without the synthetic fixture and its
committed golden is byte-identical on dev-JIT and ship-C-AOT.

Stage 6 added the derived examples gate. The comparison test loads only
`engine.generated.d.ts` and the engine native library for e09, then
asserts `dev-JIT == ship-C-AOT == expected` as byte vectors.

```text
$ cargo test --offline -p subscript-examples --test gate -- --nocapture
running 5 tests
derived example set: e09-c-structs-and-slices, e10-c-callbacks-and-handles
test derived_example_set_excludes_phase_gate_programs ... ok
test engine_mirror_regenerates_byte_identically ... ok
test two_header_gate_emits_both_provenance_vocabularies ... ok
test every_phase_gate_program_matches_dev_jit_ship_c_aot_and_golden ... ok
test every_example_matches_dev_jit_ship_c_aot_and_golden ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.75s
```

The committed e09 bytes used by the third comparison are:

```text
$ sed -n '1,40p' examples/e09-c-structs-and-slices.expected
deferred=0,0
ready=1,5,1,0
read=2
entity=1,1.25,-2.5,0.5,3,0
entity=2,11.5,22.25,1.5,9,1
flags=2,3,3
changed=2,10,2,1
frame=1,0.125
stepped=3,15,3,2
```

Result: **PASS**.

## 2. Two headers bind in one program

Criterion: the engine and fixture mirrors bind together and produce one
byte-identical result on both tiers.

Stage 6 added `examples/gate/two-header-binding.ts`. The
`every_phase_gate_program_matches_dev_jit_ship_c_aot_and_golden` result
above is the three-way byte comparison. The adjacent emission test also
asserts both includes and the distinct `EngEntityStateView`,
`EngEntityStateOut`, and `SubBufferView` aggregate spellings.

The committed bytes are:

```text
$ sed -n '1,40p' examples/gate/two-header-binding.expected
deferred=0,0
ready=1,5,1,0
read=2
entity=1,1.25,-2.5,0.5,3,0
entity=2,11.5,22.25,1.5,9,1
flags=2,3,3
changed=2,10,2,1
frame=1,0.125
stepped=3,15,3,2
fixture-deferred=0
fixture=1,9
```

Result: **PASS**.

## 3. The binding path names no fixture

Criterion: the widened §23.8.3 fixture-name search is empty under
`compiler/src` and `codegen/src`.

Stage 4 moved fixture symbol registration and native inputs to test
scaffolding. The measured search was:

```text
$ rg -n 'interop\.h|interop\.c|corpus/interop|interop_dir|register_interop|Sub(Slice|BufferView|StringView|LogCallback|WaitList)|sub(ChainPayloadValue|Device|Slice|DrawListTotal|AccessMatches|BulkConsume|CommandBufferTotal|StageMatches|FutureMake|StatsMake)' compiler/src codegen/src
```

Standard output was empty and `rg` exited 1, meaning no match.

Result: **PASS**.

## 4. Deleting the fixture leaves the binding path buildable

Criterion: deleting the fixture and its test-only compilation and
registration scaffolding requires no edit under `compiler/src` or
`codegen/src`.

Stage 4 established the deletion boundary. It was reproduced for this
record by moving the three exact fixture inputs to an ignored hold
directory, checking both library targets, and restoring them through the
shell exit trap:

```sh
set -e
p25_hold=target/p25-deletion-hold
mkdir -p "$p25_hold/codegen/tests/support"
restore_p25_fixture() {
  mv "$p25_hold/interop" corpus/interop
  mv "$p25_hold/build.rs" codegen/build.rs
  mv "$p25_hold/codegen/tests/support/native_fixture.rs" codegen/tests/support/native_fixture.rs
}
trap restore_p25_fixture EXIT
mv corpus/interop "$p25_hold/interop"
mv codegen/build.rs "$p25_hold/build.rs"
mv codegen/tests/support/native_fixture.rs "$p25_hold/codegen/tests/support/native_fixture.rs"
cargo check --offline -p subscript-compiler -p subscript-codegen --lib
```

Relevant compiler output; Cargo's machine-local absolute manifest path is
omitted because repository files may not contain developer-local paths:

```text
    Checking subscript-codegen v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.39s
```

The trap restored all three paths. `git status --short` was empty
afterwards.

Result: **PASS**.

## 5. Missing registration fails loudly on both hosts

Criterion: a missing foreign registration names the unresolved symbol on
both tiers on Unix and Windows, before a platform symbol lookup can
succeed by accident.

Stage 4 captured this Unix runner error for both dev-JIT and ship-C-AOT:

```text
unresolved foreign symbol `stage4MissingForeignSymbol`:
no supplied native library registers it
```

The focused test was re-run on the current Unix host:

```text
$ cargo test --offline -p subscript-codegen --test native_library unregistered_foreign_symbol_is_named_before_platform_lookup -- --nocapture
running 1 test
test unregistered_foreign_symbol_is_named_before_platform_lookup ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s
```

The test calls both `run_jit` and `run_c_aot` with an empty native-library
set and asserts the exact symbol name in
`RunError::UnresolvedForeignSymbol`.

Windows: **PASS** (measured 2026-07-28, `x86_64-pc-windows-msvc`, MSVC
only — clang not on `PATH`, `$CC` unset; the ship tier is now `cl`, see
`specs/tracking/windows-portability.md`):

```text
$ cargo test --offline -p subscript-codegen --test native_library unregistered_foreign_symbol_is_named_before_platform_lookup -- --nocapture
running 1 test
test unregistered_foreign_symbol_is_named_before_platform_lookup ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s
```

Both runners name the symbol before any platform lookup on Windows too:
the demand-side check is host-independent by construction (the sole
`Linkage::Import` path is verified before finalize/link), and this run is
the evidence for it, closing the concern that `cranelift-jit`'s Windows
default lookup (`GetProcAddress` over loaded modules) could resolve by
accident.

Result: **PASS** (Unix and Windows, both tiers).

## 6. Unparseable provenance is rejected at ingestion

Criterion: missing, malformed, misattached, or duplicate provenance
records reject the ambient mirror and name it; a foreign-free ambient
source remains accepted.

Stage 2 added typed ingestion and these direct tests:

```text
$ cargo test --offline -p subscript-compiler --test provenance -- --nocapture
running 6 tests
test a_malformed_record_is_rejected_with_its_mirror_name ... ok
test duplicate_records_for_one_parameter_are_rejected ... ok
test foreign_declarations_without_a_header_record_are_rejected ... ok
test a_record_naming_a_nonexistent_parameter_is_rejected ... ok
test foreign_free_ambient_source_needs_no_provenance ... ok
test well_formed_records_are_attached_to_the_typed_hir_surface ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Each rejection test asserts the mirror name and offending record content
in the diagnostic. The success test asserts typed descriptor, string-view,
callback, and mirror provenance on the HIR.

Result: **PASS**.

## 7. Regeneration is byte-identical and TypeScript-clean

Criterion: provenance remains in the committed fixture mirror,
regeneration is byte-identical, and stock `tsc` accepts the mirror.

Stage 1 added generated provenance and the regeneration assertion:

```text
$ cargo test --offline -p subscript-bindgen --test regen committed_mirror_is_byte_identical_to_regeneration -- --nocapture
running 1 test
test committed_mirror_is_byte_identical_to_regeneration ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.02s
```

The records present in the committed mirror were measured with:

```text
$ rg -c '^// @subscript-c-' corpus/interop/interop.generated.d.ts
15
```

The full TypeScript check is recorded under criterion 9.

Result: **PASS**.

## 8. Unsupported reachable callback shapes are rejected

Criterion: the three unsupported reachable shapes name the typedef and
the one supported trampoline shape, while an unsupported unreachable
typedef is omitted without rejecting the header.

Stage 1 and its reachability correction added the four required cases.
Each rejection test asserts both its concrete typedef
(`EngExtra`, `EngShort`, or `EngReturning`) and:

```text
supported shape is `void Callback(StringView message, void *userdata1, void *userdata2)`
```

Measured output:

```text
$ cargo test --offline -p subscript-bindgen --test provenance callback_ -- --nocapture
running 5 tests
test callback_with_one_userdata_slot_is_rejected ... ok
test unreachable_unsupported_callback_is_omitted_without_rejecting_header ... ok
test callback_record_names_the_c_function_pointer_typedef ... ok
test callback_with_an_extra_parameter_is_rejected ... ok
test callback_with_non_void_return_is_rejected ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.02s
```

Result: **PASS**.

## 9. Standing gates and clippy baseline

Criterion: the full differential gate and `tsc` are green, and clippy
matches its documented baseline.

`specs/tracking/p20-trap-site-ir.md` and
`specs/tracking/p24-monotonic-costs.md` document the command-specific
baseline as 16 warnings for `subscript-codegen`. The same command
produced:

```text
$ cargo clippy --offline -p subscript-codegen
warning: `subscript-runtime` (lib) generated 21 warnings (run `cargo clippy --fix --lib -p subscript-runtime -- ` to apply 2 suggestions)
warning: `subscript-compiler` (lib) generated 7 warnings (run `cargo clippy --fix --lib -p subscript-compiler -- ` to apply 4 suggestions)
warning: `subscript-codegen` (lib) generated 16 warnings (run `cargo clippy --fix --lib -p subscript-codegen -- ` to apply 6 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.48s
```

The codegen count equals the documented 16-warning baseline.

The final workspace gate ran without a pipeline and exited 0:

```text
$ cargo test --offline --workspace --no-fail-fast
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 116 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.44s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 57.66s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 70.85s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.29s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.15s
test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s
test result: ok. 187 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.70s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The 39 targets report 632 passed tests and one ignored test. The
89-golden differential target is included in the output above and
passed.

```text
$ npx tsc -p tsconfig.json
```

The TypeScript command produced no output and exited 0.

Result: **PASS**.

## Phase cost

Every generated mirror with foreign declarations now carries provenance
directives. The committed fixture mirror contains 15 directive lines:
one header, one callback typedef, one string view, and twelve descriptor
records. This is 15 additional lines paid by that host mirror.

## Carried forward

The engine facade selects `__declspec(thread)` under MSVC and
`_Thread_local` elsewhere. That MSVC path was **measured 2026-07-28**: the
examples gate compiles `engine.c` with `cl` and passes on
`x86_64-pc-windows-msvc` (`specs/tracking/windows-portability.md`), so the
`__declspec(thread)` frame record is exercised. No longer unmeasured.

## Deliberate non-scope

P25 does not generalize the runtime callback trampoline. Per §23.3a, the
emitted C still assumes exactly
`void Callback(StringView, void *, void *)`; bindgen rejects every
reachable callback typedef outside that shape. Supporting arbitrary
callback signatures requires a later contract and lowering.

## Phase Review — 2026-07-28

A fresh-context reviewer read the cumulative diff against the contract and
ran the pre-registered inherited-precedent audit in the same pass.
Findings: **1 MAJOR, 6 MINOR, 0 CRITICAL.** All closed; the audit's three
hits are recorded in `specs/tracking/inherited-precedent-audit.md`.

**The MAJOR is the one worth keeping.** A foreign function returning a
string view or a `(pointer, count)` descriptor **by value** passed bindgen,
passed the checker, and reached the dev tier as a single `I64` return
against a callee returning a 16-byte aggregate in two registers — while the
ship tier failed with a C type error. A silent dev-tier mis-marshal plus a
loud ship-tier failure is a **tier divergence**, which is what the standing
gate exists to prevent, and no test could see it because neither the
fixture nor the facade declares such a function.

The cause is stated plainly so the next contract does not repeat it: §23.3
specifies provenance **per parameter**, and the record vocabulary
(`function=… parameter=…`) has no way to name a return. That was not a
considered exclusion. It was written while thinking about arguments.

The remaining MINORs were unconsumed provenance surface, three more header
shapes bindgen wrote and the toolchain then refused, two untested rejection
paths, a misquoted diagnostic in two examples, and the fixture still being a
crate-build input — now its own dev-dependency-only crate, so §23.8.4's
deletion touches test scaffolding in fact rather than by letter.

Post-review gate: exit 0, 41 targets, 644 tests, the 89-golden differential
gate green, no golden and no mirror moved, `tsc` clean.

**Criterion 5 is now PASS on both hosts** (2026-07-28): the Windows half
was run on `x86_64-pc-windows-msvc` after the ship tier moved to `cl`
(`specs/tracking/windows-portability.md`), and both runners name the
unresolved symbol. With criterion 5 settled and no open CRITICAL/MAJOR,
**the phase is COMPLETE**.

