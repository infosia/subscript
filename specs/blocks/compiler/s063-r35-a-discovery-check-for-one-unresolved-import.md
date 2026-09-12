<!-- §63 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 63. R35 — a discovery check for one unresolved import

Origin: downstream request R35, 2026-08-22, at pin `ba6aa2e`. The
downstream generates a support module `<stem>.typegpu.ts` from the
schemas in `<stem>.ts`, and the program imports that module. The
generator must read the program's HIR before the module exists.
Today the first `check_program` fails at the import and returns no
HIR, so the downstream scans the import statement with a second
parser, which its own rules forbid.

Measurements at the pin, on this host:

1. `resolve_imports` (`compiler/src/check/mod.rs`) reports S100
   "imported module `./x` is not among the program's files" and
   binds nothing; the check returns `Err`.
2. `Type::Error` is "assignable everywhere so one error does not
   cascade" and is never in a successful check's HIR
   (`compiler/src/types.rs`). `unknown name` (expressions) and
   `unknown type name` (types) are the two lookup-failure sites.
3. `parse_import_specifiers` (`compiler/src/parse.rs`) returns the
   module specifiers only, not the imported names.

### 63.1 Rule

1. New public API in `subscript_compiler`:

   ```rust
   #[non_exhaustive]
   #[derive(Debug, Clone, Default)]
   pub struct CheckOptions {
       /// Import specifiers to bind as poisoned when absent.
       pub poison_missing_modules: Vec<String>,
   }
   pub fn check_program_with(files: &[SourceFile], options: &CheckOptions)
       -> Result<hir::Module, Vec<Diagnostic>>;
   ```

   `check_program(files)` equals `check_program_with(files,
   &CheckOptions::default())`.
2. A specifier in the option and an import's source match after the
   normalization `resolve_imports` applies (strip a leading `./` and
   a trailing `.ts`). A listed module that is among the files
   resolves as today; the option has no effect on it.
3. When a listed module is absent, every **named** specifier of that
   import binds its local name as poisoned, with no diagnostic. A
   poisoned name types as `Type::Error` in expression position and
   resolves to `Type::Error` in type position, with no diagnostic in
   either. A default or namespace specifier keeps its S100.
4. Every other rule is unchanged. A diagnostic elsewhere still fails
   the check.
5. `hir::Module` gains `poisoned_imports: Vec<PoisonedImport>`, one
   per poisoned import statement in source order:

   ```rust
   pub struct PoisonedImport {
       /// The specifier as written in the import.
       pub module: String,
       /// `(imported, local)` name pairs in source order.
       pub names: Vec<(String, String)>,
       pub pos: Pos,
   }
   ```

   The vector is empty under `CheckOptions::default()`.
6. A module with a non-empty `poisoned_imports` is a discovery HIR.
   It can hold `Type::Error`. Both codegen entry points (`emit_c`
   and the dev-tier module lowering) return an error for it; the
   caller never lowers it.

### 63.2 Changes by site

- `compiler/src/lib.rs`: `CheckOptions`, `check_program_with`;
  `check_program` delegates. `check::run` takes the options.
- `compiler/src/check/mod.rs`: `ScopeItem::Poisoned`; the absent-
  module branch of `resolve_imports` consults the option; the
  record is assembled into the module.
- `compiler/src/check/expr.rs`, `compiler/src/check/tyres.rs`: the
  two lookup sites return `Type::Error` without a diagnostic for a
  poisoned name.
- `compiler/src/hir.rs`: `PoisonedImport`, the `Module` field.
- `codegen`: the guard in rule 6.

### 63.3 Tests and gate (pre-registered exit criteria)

No corpus entry: the surface is a Rust API, not language syntax.
Unit tests in `compiler/tests/` and `codegen/tests/`, same commit:

1. A program imports `{ A_SIZE, B_WGSL }` from an absent
   `./p.typegpu`, declares a `@CStruct` class and an exported
   function that uses both names (one as a value, one as a type).
   With the option: `Ok`, the class is intact,
   `poisoned_imports == [("./p.typegpu", [("A_SIZE","A_SIZE"),
   ("B_WGSL","B_WGSL")])]`. Without the option: `Err` with the S100
   in item 1.
2. `{ A as B }` records `("A", "B")`.
3. A listed module that is present resolves normally and the record
   is empty.
4. A second, unrelated diagnostic with the option set still returns
   `Err`.
5. `emit_c` and the dev-tier lowering reject a discovery HIR.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; every corpus count and
   golden unchanged.
