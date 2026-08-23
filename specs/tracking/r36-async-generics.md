# R36 — async methods on generic classes, generic async functions

Status: **landed 2026-08-23** against `specs/blocks/compiler.md`
§64. Origin: downstream request R36. Contract `4652964`,
implementation `1438b76`.

## The request

The downstream wraps a `GPUBuffer` in a generic class `Buffer<T>`.
The typed read-back awaits a map, so the method is `async`. The
checker rejected an async method on a generic class template (r104,
§37.1) and a generic async function in await position (§26.1).

## Findings on this host, at `bb9dadc`

- `collect_class` reported S100 "async methods on generic class
  templates are not in the decided surface" at the method.
- The await path accepted `ScopeItem::Func` only; `await
  first<u32>(items)` reported S100 "`first` is not a directly
  declared async function".
- `instantiate_fn` marked an instance `exported` when the template
  was exported. The ship tier emitted the sync instance of `export
  function f<T>(): void` as `subscript_export_f_u32_`. The runner
  kicks every exported async function with no parameters as a root
  in both tiers, so an exported async instance would run twice.
- `tsc` 5.9.2 accepts both R36 shapes and an async arrow function;
  it rejects `async constructor()` (TS1089), as the parser does.

## What landed

- `collect_class` collects a generic template with async methods as
  it collects one with sync methods. The §37.1 rejections run on the
  instance in `check_class_body` (unit test: a generic `@CStruct`
  class with an async method is the r103 S100 at instantiation).
- The await path accepts `ScopeItem::GenericFunc`: without type
  arguments, S100 "generic function `first` requires explicit type
  arguments"; with them, `instantiate_fn` and the instance continues
  as a named async function. A floating `first<u32>(items)` is S013
  "async call `first<u32>(...)` must be immediately awaited".
- `instantiate_fn` passes `exported = false` for every instance
  (rule 5). No `codegen/` change: the `exported` flag already gates
  the export symbol and the root kick in both tiers.
- `language_reference.rs`: the Q34 prose names the two generic
  forms; the corpus list replaces r104 with a143 and r140;
  `generated-docs/` regenerated.
- Corpus: `a143-async-generic` (accept; `Box<T>.read`, `first<T>`,
  and `export async function tick<T>()` with `u32` and a `@CStruct`
  `Vec2`; the golden shows `tick` once), `r140-async-lambda`
  (reject, `tsc-clean-standalone`). `r104` deleted with its harness
  row and its case in the R13 unit test in `compiler/src/lib.rs`.
- Tests: `compiler/tests/async_generic.rs` (five tests, §64.3 item
  4), `codegen/tests/cemit.rs` `generic_async_instance_has_no_host_wrapper`.
- Counts: accept `.ts` 141 → 142, `.expected` 142 → 143, rejects
  135 → 135, `corpus_warn.rs` source files 143 → 144.

## Gates (this host, at `1438b76`)

- `cargo test --offline --workspace`: 59 suites, 990 passed, 0
  failed, 1 ignored, in both profiles.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- No pre-existing golden or `.expected` changed.
- The HANDOFF probes 1 and 2 run on the dev tier and print `7` and
  `3`; the exported instance probe prints `go` once.

## Review (fresh no-context subagent)

No CRITICAL or MAJOR findings. The reviewer ran the compiler and
codegen suites, `cargo fmt --check`, and stock `tsc` on `a143` and
`r140`, and traced every consumer of `Function.exported` in
`codegen/` (lowering, C emission, reload hash) to confirm rule 5.

MINOR, fixed before the implementation commit:

- `Checker.exported_fns` was written and never read after the
  change; removed.
- The `a143` purpose line did not name the two shapes; rewritten.
- `specs/blocks/collisions.md` C8 and Q34 still said "non-generic";
  a one-line R36 note added at both places.

Noted, not changed: a generic `@CStruct` class with an async method
reports the r103 S100 once per instantiation at the same position,
as every per-instance body error does.

## windows-msvc (measured at `ac9436f`)

- `cargo test --offline --workspace`: 58 suites, 973 passed, 0
  failed, 1 ignored, in both profiles. The 17 tests fewer than the
  clang host are the `offsetof` proof and the interop-fixture
  entries. The suite count is 58, not the 59 the gate list above
  records; the `offsetof` binary runs on both hosts and reports 0
  tests on windows-msvc, so the two hosts hold the same binary
  count. Correct the clang number at the next run there.
- `cargo build --offline --workspace --all-targets`: 0 warnings, in
  both profiles.
- `cargo fmt --check`: exit 0. `tsc` 5.9.2 gate: exit 0.
- The golden sweep compared 94 entries and skipped 49. `a143` is in
  the compared set: dev-JIT, ship-C-AOT, and the golden agree byte
  for byte under MSVC `cl` 19.44.35222, `/std:c11 /O2`. The golden
  shows `tick` once, so rule 5 holds on this host.
- `compiler/tests/async_generic.rs`: 5 passed.

### Clippy against the recorded baseline

`cargo clippy --offline --workspace --all-targets` exits 0. Library
counts against the `p25-header-deprivileging.md` §9 baseline:
`subscript-compiler` 7 (baseline 7), `subscript-runtime` 22
(baseline 21), `subscript-codegen` 29 (baseline 16). Every added
warning is a style or complexity lint —
`clippy::too_many_arguments` (11 of the 13 in `codegen`),
`match_like_matches_macro`, `useless_conversion`,
`doc_lazy_continuation`, `type_complexity`, `manual_clamp`,
`manual_is_multiple_of`, `collapsible_match`. No correctness lint
and no `unsafe` lint appears. The R-series commits did not restate
the baseline, so the drift has no single owner. Record a new
baseline, or reduce the counts to the old one.
