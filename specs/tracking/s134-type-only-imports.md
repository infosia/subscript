# Type-only imports (§134)

## Change

The scope entry carries a `type_only` flag beside the §127 `imported`
flag. `resolve_imports` sets it where it registers the import, from
`type_only_import` (`compiler/src/check/signatures.rs`): the declaration
is `import type`, or the specifier is written `type X`.

`compiler/src/check/lookup.rs` has three lookups:

- `scope_item(name, pos)` is the value lookup. Every value use calls it
  with the position of the identifier. For a type-only binding it
  reports S100 at that position and answers `Poisoned`. A set of
  reported positions keeps one diagnostic per use site when a site
  looks the name up twice: a static method call falls back to the
  member call path, and a compound assignment (`g += 1`) or an update
  (`g++`) reads and writes the same identifier.
- `peek_scope_item(name)` answers the value view with no report. It
  serves existence and shadow tests only: `ambient_visible`, the
  `new Worker` shadow test, the static generic method pre-test, and the
  read-before-declaration shadow test. A type-only binding answers
  `Poisoned`, so no value path can reach its declaration.
- `type_scope_item(name)` is the type lookup. `compiler/src/check/tyres.rs`
  uses it only. A type-only binding resolves to its declaration.

The value uses that call `scope_item`: an identifier read
(`check_ident`, which also serves function values, template reads, and
spreads), a named call, an awaited call, `new`, a namespace member read
(`E.A`, `Box.s`), a static method call, a static field write, an
identifier assignment target, a `Worker.spawn` entry, and the right
operand of `instanceof`.

`check_instanceof` (`compiler/src/check/exception.rs`) resolves an
Error-family right operand through `error_name_is_ambient`. Before this
fix, a type-only binding there gave the S100 "`instanceof` requires an
Error-family class as its right operand" at the binary expression, with
the `InstanceofNonError` block. That block states that TypeScript
accepts the form, which is false here: `tsc` gives TS1361 at the right
operand. Now a type-only binding as the right operand goes through
`scope_item`: one rule 2 S100 at the operand, and no `instanceof`
diagnostic. The left operand is still checked. A name that is not a
type-only binding keeps the previous path.

An identifier assignment target reports the type-only S100 and no §127
S100: the value lookup runs first and a poisoned item ends the target.
`tsc` reports both TS1361 and TS2632 at that site. The same holds for
`g += 1` and `g++`: one S100 at the identifier.

All edited Rust files stay below 2,000 lines. No split was required.
The largest edited file, `compiler/src/check/mod.rs`, has 1,755 lines.
`generated-docs/corpus-index.md` gains the r282 rows.

## Red

Pin: `adc2841c`. The tree was clean. `cargo build --offline --locked
-p subscript-cli --bin subscript` was up to date. That binary ran a
copy of `corpus/reject/r282-type-only-import-value/` (`subscript run
main.ts`) with exit 0 and stdout:

```text
1
```

After the change the same command exits 1:

```text
error[S100]: `Box` cannot be used as a value because it was imported with `import type`
 --> main.ts:10:19
```

## TypeScript measurement

TypeScript 5.9.2 ran with the `tsc_corpus.rs` options (strict, `noEmit`,
ES2022, ESNext modules, Bundler resolution, the ES2022 and
ESNext.Disposable libraries, `corpus/interop/*.d.ts`, and
`prelude/lang.d.ts`).

- r282: `main.ts(10,19): error TS1361`.
- `new` through `import { type B2 }`, a call, a template read, `E.A`,
  `Box.s`, `Box.make()`, a function value, `new` in a return
  statement: one TS1361 each, at the same line and column as the
  checker's S100.
- `Box.s = 5`, `[0, ...arr]`, `Worker.spawn(entry)`, `await af()`: one
  TS1361 each.
- `g = 5`: TS1361 and TS2632 at one position.
- `const b: Box | null = null` through `import type`: accepted.

## Rule 5

Measured at `adc2841c` through `subscript run`:

- `mid.ts`: `import type { Box } from "./lib"; export { Box };` and
  `main.ts`: `import { Box } from "./mid"` with `new Box()`. The checker
  accepted the program, and it printed `7`. `tsc` accepts `mid.ts` and
  reports TS1361 at `new Box()` in `main.ts`: the re-export carries
  the type-only fact.
- The same re-export in the entry module was S100 "the entry module
  exports functions only" (C18 block). After the change it is the
  rule 5 S100 alone.

So the re-export gave a type-only name a value meaning. It is now the
§128 rule 7b S100 with the C18 block (`Divergence::NamedModuleSurface`),
at the export specifier, not a resolution diagnostic. The export name is
poisoned, so an importer of it reports nothing more (§128 rule 5b). The
check reads the import declarations of the module through
`type_only_import`, because `collect_named_exports` runs before
`resolve_imports` registers the scope entry.

## Open item: uninstantiated generic bodies

The checker does not check the body of a generic function that no call
instantiates. This gap predates §134. Measured with
`import type { mk }`: `function gen<T>(x: T): i32 { return mk(); }`
with no instantiation is accepted. With `gen<i32>(1)` in the program,
the S100 is reported at `mk()`. The gap is its own open item, not a
§134 defect.

## Tests

`compiler/tests/type_only_imports.rs`: 9 tests pass in 0.03 seconds.

- Eleven rule 2 use forms (a call, `new`, a template read, `E.A`,
  `Box.s`, `Box.make()`, a function value, a static field write, a
  spread, a `Worker.spawn` entry, an awaited call), each under both
  spellings: exactly one S100, with the message, no divergence block,
  and the use position. Each form through an ordinary import is
  accepted.
- An identifier assignment, `g += 1`, and `g++`: the exact list is one
  S100 at the identifier under both spellings; the ordinary import
  control gives the §127 S100 only.
- A renamed type-only import reports the local name.
- `import { type Box, mk }` keeps `mk` a value.
- Rule 3: an annotation, a type argument, a return type, a parameter
  type, an array element type, `GBox<Box>`, `Promise<Box | null>`, a
  class field type, `Set<E>`, and `Worker<M, M>` accept both spellings.
- Rule 4: an unused type-only import of a function and a global is
  legal. A type-position use of each gives the exact list of the
  ordinary import control: two S016 "unknown type name", one for `g`
  and one for `mk`.
- `instanceof`: a type-only `Box` as the right operand, with an
  Error-typed parameter and with a catch binding as the left operand,
  gives the exact list of one rule 2 S100 at the operand, no divergence
  block. The ordinary import control gives the `InstanceofNonError`
  S100. `RangeError` beside a type-only import stays accepted. With the
  fix disabled, the test fails: the list is the `instanceof` S100 at
  the binary expression.
- Rule 5: three re-export forms give one S100 at the specifier with the
  C18 divergence; the ordinary import control is accepted.
- Rule 5 in the entry module: `import type { mk }` with `export { mk };`
  gives the rule 5 S100 only, under both spellings. The ordinary
  import control gives the entry export S100 only ("host entries must
  return void").

`compiler/tests/corpus_reject.rs` has the row
`("r282-type-only-import-value/main.ts", RuleCode::S100, 10)`.
No existing `.expected` file changes.
