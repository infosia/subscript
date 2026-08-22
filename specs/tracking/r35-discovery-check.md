# R35 — a discovery check for one unresolved import

Status: **landed 2026-08-22** against `specs/blocks/compiler.md`
§63. Origin: downstream request R35. Contract `391e7eb`,
implementation `254fb0c`.

## The request

The downstream generates `<stem>.typegpu.ts` from the schemas in
`<stem>.ts`; the program imports that module. The generator must
read the program's HIR before the module exists. `check_program`
failed at the import and returned no HIR, so the downstream scanned
the import statement with a second parser.

## Findings on this host, at `ba6aa2e`

- `resolve_imports` reported S100 "imported module `./x` is not
  among the program's files" and bound nothing.
- `Type::Error` was never in a successful check's HIR.
- `parse_import_specifiers` returned module specifiers only.

## What landed

`CheckOptions { poison_missing_modules: Vec<String> }`
(`#[non_exhaustive]`, `Default`) and `check_program_with(files,
&options)`; `check_program` delegates with the default. A listed
absent module binds each named specifier as `ScopeItem::Poisoned`:
`Type::Error` in expression and type position, no diagnostic. The
arguments of a poisoned call or `new`, and the type arguments of a
poisoned generic reference, are still checked, so an unrelated
diagnostic still fails the check. A default or namespace specifier
keeps S100 "only named imports are in the decided surface"; a bare
`import "./x"` of a listed absent module keeps S100 "is not among
the program's files". `hir::Module.poisoned_imports` records the
specifier as written, the `(imported, local)` pairs in source order,
and the position of the specifier string. Specifier matching
normalizes both sides (strip `./`, strip `.ts`).

Guards (rule 6 widened): `emit_c`, the dev-tier lowering,
`value_class_layouts`, and `padding_ranges` return `Err` "cannot
{emit|lower|lay out} discovery HIR: poisoned import `./p.typegpu`".
Every other codegen entry takes `&[SourceFile]` and runs the default
check, so a discovery HIR cannot reach it.

No corpus entry. Tests: `compiler/tests/discovery_check.rs` (the
§63.3 items 1–4, a default-import rejection, argument and
type-argument diagnostics through a poisoned name),
`codegen/tests/discovery_check.rs` (`emit_c` and
`value_class_layouts`), and the dev-lowering guard test in
`codegen/src/lower/mod.rs` (`lower_module_with` is `pub(crate)`).

## Gates (this host, at `254fb0c`)

- `cargo test --offline --workspace`: 57 suites, 984 passed, 0
  failed, 1 ignored, in both profiles.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` gate: exit 0.
- No corpus, golden, `.expected`, or generated-docs change.

## Review (fresh no-context subagent)

Two MAJOR, fixed before the commit: a poisoned callee or
constructor returned before its arguments were checked, and a
poisoned generic reference did not resolve its type arguments; both
hid an unrelated diagnostic. MINOR, fixed: the layout entry points
had no guard; `emit_c` docs; a duplicated message literal; an empty
`PoisonedImport` record; two doc sentences. MINOR, recorded and not
fixed: the dev-lowering guard test sits in `codegen/src/lower/mod.rs`
and not in `codegen/tests/`.

## windows-msvc (measured at `9c6195d`)

R35 adds no corpus entry and no platform-dependent code. Gates on
this host:

- `cargo test --offline --workspace`: 57 suites, 967 passed, 0
  failed, 1 ignored. Both new suites pass:
  `compiler/tests/discovery_check.rs` (8 tests) and
  `codegen/tests/discovery_check.rs` (2 tests).
- `cargo build --offline --workspace --all-targets`: 0 warnings.
  `cargo fmt --check`: exit 0. `tsc` 5.9.2 gate: exit 0.
- Guard coverage: `emit_c`, `emit_c_without_main`, the dev-tier
  lowering, `value_class_layouts`, and `padding_ranges` all reject a
  discovery HIR. `emit_c_without_main` shares the `Emitter::new`
  guard; the two layout entries share `reject_discovery_hir`.
