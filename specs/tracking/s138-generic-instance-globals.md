# Generic instance module globals

Contract: `specs/blocks/compiler/s138-a-generic-instance-body-sees-every-module-global.md`.

## Red evidence

Pin: `4d6c4962b241259486c8c21a4085260a7f6a1300`.
The CLI binary was built at the pin before the compiler changes.
Entry: `corpus/accept/a302-generic-instance-globals.ts`.

```text
check: corpus/accept/a302-generic-instance-globals.ts: no errors
subscript: internal lowering error: LIR construction failed: :0:0: produced invalid LIR:
function 2 (`peek`): block 0 instruction 0 operand 0 has an invalid constant/type pairing: Constant { ty: Error, kind: Null }
```

Node v24.18.0 with TypeScript 5.9.2 prints `7` (bytes `370a`).

## Cause and change

Pass A binds every module name.
Pass B resolves each declaration signature in source order.
A generic type annotation starts instance body checks before Pass B completes.
The global lookup returns `Type::Error` when the bound global has no signature, without a diagnostic.
The later opaque check sees the complete signatures and reports no error at that site.

Instance bodies now wait until every module signature resolves.
Deferred instance-body diagnostics now follow Pass B diagnostics.
The deferred check restores the template file, type arguments, and container context.
The existing initializer scan supplies S100 and the instance member route.
The existing opaque diagnostic merge preserves S016 for an unknown name.

## Acceptance evidence

The checker tests assert the complete S100 messages for methods, constructors, field initializers, and both accessor forms.
They assert the complete S016 message for each body form.
The codegen controls lower and execute the generic function and imported generic instance; each output is `7`.
The generic-function twin calls `peek<i32>` from a global initializer before `m` and asserts the complete S100 message.

The corpus interpreter harness compares the new entry output with the new golden.
The dedicated module-initializer test does not run the same entry again.
The corpus loaders derive the entry set from the corpus directory; no entry table needs an edit.
The document generator adds the new entry to `generated-docs/corpus-index.md`.
No existing `.expected` file changes.

## Pre-gate verification

- The checker tests cover five early-read routes and four unresolved-name body forms.
- The codegen controls check the complete lowering and interpreter execution; each output is `7`.
- The development JIT and ship C probe each print `7`.
- The pinned `cargo fmt --check` passes.
- `cargo build --workspace --all-targets` passes with zero warnings.
- `cargo clippy --workspace --all-targets` passes; no warning points to a changed file.

The full gate record supplies the final verdict and all corpus comparisons.

## Gate result

`tools/hygiene.sh` exits 0.
`tools/gate.sh full` ran once and exits 0.
Record: `target/gate/20260930T215205Z-full.md`.

```text
gate full 4d6c4962b241259486c8c21a4085260a7f6a1300 dirty:9 debug 2088/0/3 release 2085/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The debug and release gates include the new entry in the interpreter, JIT, ship C, TypeScript, and node corpus checks.
The gate reports no existing golden changes.
No missing input-form fact or requirement outside the contract was found.

## Review corrections

Deferred instance-body diagnostics follow Pass B diagnostics.
The two execution controls are in `codegen/tests/generic_instance_globals.rs`.
The accepted generic-function control has an early-read twin with the same declarations and call.
The twin asserts S100: `` `m` is accessed before its declaration, through `peek<i32>` ``.
Both execution controls print `7`; the early-read twin reports the expected S100.
The two checker tests pass.
The module-initializer suite passes 25 tests in 1.15 seconds, with 7 ship-C program compiles.
The deferred-body check uses `get` and `let-else`; missing metadata emits a diagnostic.

## Review-correction gate

The pinned `cargo fmt --check` passes.
`cargo build --workspace --all-targets` passes with zero warnings.
`tools/hygiene.sh` exits 0.
`tools/gate.sh full` ran once for these corrections and exits 0.
Record: `target/gate/20260930T223433Z-full.md`.

```text
gate full 4d6c4962b241259486c8c21a4085260a7f6a1300 dirty:10 debug 2087/0/3 release 2084/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```
