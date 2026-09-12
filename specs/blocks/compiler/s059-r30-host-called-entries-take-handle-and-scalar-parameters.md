<!-- §59 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 59. R30 — host-called entries take handle and scalar parameters

Origin: downstream request R30, 2026-08-16, at pin `e8e01d9`. The
engine-embedded class moves from pull (`engineAcquireDevice` inside
the entry) to push: the host passes long-lived handles into
entries. The downstream asks for the pin; the borrow discipline
above the language stays theirs.

Measurements at the pin, on this host:

1. The checker accepts handle-typed and reference-class-typed
   parameters on exported functions (probe, exit 0).
2. The ship tier emits `subscript_export_<name>` only for a
   zero-argument `void` export (`emit_exports`,
   `codegen/src/cemit.rs`). A parameterized export compiles to a
   `static` function with no host symbol. `a23`'s
   `update(dtFixed: f32)` runs only because `main` calls it
   in-script.
3. The dev session's host surface,
   `ReloadSession::call_export`, resolves only zero-argument
   `void` entries (`codegen/src/reload.rs`).
4. The AOT host hooks receive `subscript_rt_context*`
   (`codegen/src/aot.rs`), so a fixture hook can call an export
   wrapper directly. No new ship-harness machinery is required.

### 59.1 Rule

1. An exported function is **host-callable** when it is
   synchronous, returns `void`, and every parameter is a boundary
   scalar (sized numeric, `boolean`) or an opaque handle.
   Zero-argument `void` async exports stay host-callable,
   unchanged.
2. For every host-callable export, the ship tier emits
   `void subscript_export_<name>(subscript_rt_context* ctx, ...)`
   with the same parameter C types as the internal function.
3. The dev session gains
   `call_export_with(name, &[EntryArg]) -> Result<(), RunError>`.
   `EntryArg` covers opaque handles and the boundary scalars. An
   unknown name, an arity mismatch, or an argument-kind mismatch
   fails with `RunError::Internal`, and no script code runs.
   `call_export` keeps its zero-argument behavior.
4. An exported function that is not host-callable stays a legal
   script export with no host symbol. The checker changes nothing
   and rejects nothing new, so this cycle has no reject-corpus
   entry. The convention comments in the emitted C name the
   host-callable subset.
5. At the C level, a parameter is a borrow for the duration of
   the call. Handle values are copyable; the script can wrap and
   store them, and the borrow discipline above the language is
   the host's.
6. Parameter marshaling equals the foreign-call marshaling for
   the same types.

### 59.2 Changes by site

- `codegen/src/cemit.rs` (`emit_exports`): emit the parameterized
  wrappers.
- The host-export convention text lives in
  `runtime/src/host_header.rs` and reaches the emitted C through
  the generated `runtime/include/subscript_runtime.h` and
  `AOT_ENTRY_C` (`codegen/src/aot.rs`). Update the generator;
  regenerate the header. *(Correction: the contract first
  attributed this text to `cemit.rs`; the implementer measured
  the real sites.)*
- `codegen/src/reload.rs`: the entries table records each
  host-callable export's parameter signature; add `EntryArg` and
  `call_export_with`. `codegen/src/lib.rs` re-exports `EntryArg`.
- The JIT generation site that fills the entries table records
  the signatures.
- `codegen/src/bin/capture.rs` and `codegen/tests/golden.rs`:
  drive `a137` on the a128 pattern — dev through
  `call_export_with` then `call_main`; ship through a pre-entry
  hook that calls the export wrapper.
- `corpus/interop/interop.c`: a hook
  `subHostOwnedStateAdoptDrive(subscript_rt_context*)` borrows
  the host state, advances it once, and calls
  `subscript_export_adopt(ctx, state, 7)`. The hook lives in
  `interop.c` only — never in `interop.h`; the mirror must not
  move.
- `codegen/tests/support/native_fixture.rs`: bindings so the dev
  path obtains the same handle and advances it once before
  `call_export_with`.
- No checker, runtime, or prelude change.

### 59.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the emitted C for a parameterized
export contains no `subscript_export_` wrapper for it (record the
grep), and the dev session has no argument-taking call. Record
both before the implementation.

1. `corpus/accept/a137-handle-entry-param.ts` + `.expected`:
   `export function adopt(state: SubHostOwnedState, tag: i32)`
   wraps the handle in a reference class, stores it in a module
   global, and prints the tag; `main` advances twice through the
   stored wrapper and prints the counts. The host side advances
   once before it calls `adopt`, so the printed sequence proves
   the same host object crossed the parameter. Golden from the
   dev JIT; ship byte-identical.
2. Unit tests in the same commit: the emitted C for a probe
   program contains the parameterized wrapper with handle and
   scalar C types; a parameterized async export gets no wrapper;
   `call_export_with` fails on wrong arity and on a wrong
   argument kind; `call_export` still runs a zero-argument entry.
3. Counts: accept single files 135 → 136; accept source files
   137 → 138; goldens 136 → 137; rejects unchanged at 126. The
   generated docs regenerate byte-exact.
4. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical; the
   interop mirror regenerates byte-identically.
