# R30 — host-called entries take handle and scalar parameters

Status: **landed 2026-08-16** against `specs/blocks/compiler.md`
§59. Origin: downstream request R30 (with R31 in one handoff;
their pin `dae6e10`). Contract `8d50f0c` + correction `99eacce`,
implementation `c6cb43c`.

## The request

The engine-embedded class moves from pull to push: the host passes
long-lived handles into entries
(`export function init(device: GPUDevice, queue: GPUQueue)`).
The downstream asked for the pin and kept the borrow discipline on
its side.

## Findings on this host, at `e8e01d9`

- The checker accepts handle-typed and class-typed parameters on
  exported functions, but `emit_exports` wraps only zero-argument
  `void` exports and `ReloadSession::call_export` resolves only
  those. A parameterized export has no host symbol on either tier;
  `a23`'s `update(dtFixed: f32)` runs only because `main` calls it
  in-script.
- The AOT host hooks receive `subscript_rt_context*`, so a fixture
  hook drives the new wrapper with no new harness machinery.
- The host-export convention text lives in
  `runtime/src/host_header.rs`, not in `cemit.rs`; the implementer
  measured this and the contract was corrected (`99eacce`).

## What landed

Host-callable = exported, synchronous, `void` return, every
parameter a boundary scalar or an opaque handle; zero-argument
`void` async exports stay host-callable. The ship tier emits
parameterized `subscript_export_<name>` wrappers. The dev session
gains `EntryArg` and `call_export_with`, dispatched through a
uniform `(ctx, values)` reload adapter; a swap refreshes the
entries map per generation. Name, arity, and argument-kind
mismatches fail before any script code runs.

Corpus: `a137-handle-entry-param`. The fixture advances the
host-owned state once and calls
`subscript_export_adopt(ctx, state, 7)`; the script wraps and
stores the handle, then advances twice. The printed `41`/`42`
sequence proves the same host object crossed the parameter,
byte-exact on both tiers. The drive hook lives in `interop.c`
only; `interop.h` and the mirror did not move. A weak
`subscript_export_adopt` fallback keeps the other fixture links
whole; the fixture never builds on windows-msvc (§11c), so the
GCC/clang attribute has no MSVC exposure.

## Known approximation

`is_opaque_handle` (`codegen/src/lower/mod.rs`) is structural: a
non-boundary reference class with no fields, constructor, methods,
or index signature. An empty script class therefore classifies as
a handle parameter and its export gains a wrapper. The checker's
`handle_classes` set does not reach HIR. Honest programs are
unaffected; precision needs an `is_handle` flag on
`hir::ClassDef`. Deferred until evidence of harm.

## Red, at the contract pin

The emitted C for `adopt(state, tag)` held only
`static void subscript_fn_adopt(...)` — no `subscript_export_`
wrapper — and `call_export` failed every non-zero-argument entry
with "is not an exported zero-argument void function".

## Gates (this host, at `c6cb43c`)

- `cargo test --offline --workspace`: 55 suites, 952 passed, 0
  failed, 1 ignored, exit 0. The same counts in the release
  profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Every pre-existing golden, `.expected`, and the interop mirror
  byte-identical; the only new golden is a137's (137 total).

## windows-msvc break, found and fixed 2026-08-17

The first windows-msvc run of R30 failed. Measured at `1f875da`
with the pinned toolchain, in the dev profile and the release
profile:

```
thread 'r30_handle_entry_parameters_match_the_golden_across_tiers'
panicked at codegen\tests\golden.rs:168:48:
R30 uses the native fixture
test result: FAILED. 31 passed; 1 failed
```

The fixture never builds on windows-msvc (§11c), so
`native_libraries` returns `None` there. Every other
fixture-dependent test in `golden.rs` skips on `None`. The R30
test called `.expect` instead. The golden sweep skipped a137
correctly, so the sweep stayed green and one test carried the
whole break.

The `HANDLE_ENTRY_PARAM_ID` branch of `run_dev_corpus_entry` in
the same commit carries the `#[cfg(not(all(windows, target_env =
"msvc")))]` guard. The exclusion is missing in one place only:
the test body.

The fix gives that test the skip pattern of the file. It adds no
`#[cfg]`, so a host that builds the fixture still runs both tiers
against the golden.

Gates after the fix, on `x86_64-pc-windows-msvc` at `1f875da`:

- `cargo test --offline --workspace --no-fail-fast`: 55
  harnesses, 942 passed, 0 failed, 1 ignored, exit 0. The same
  counts in the release profile.
- `cargo build --offline --workspace --all-targets`: 0 warnings
  in each profile.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- Golden sweep: 91 entries compared, 48 skipped, against 139
  `.expected` files.

This is the fourth instance of the reference-platform class after
§11c.3, §44.10, and §55. Rule: a test that uses the interop
fixture must skip on `None`, never expect it.
