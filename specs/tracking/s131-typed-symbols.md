# §131 — A declaration symbol is a type

Contract: `specs/blocks/compiler/s131-a-declaration-symbol-is-a-type.md`.

## The type

`hir::Symbol` is in `compiler/src/hir/names.rs`. It holds the identity text in a private field.
`source_name()` returns the source spelling. `full_text()` returns the complete identity text.
`from_full_text()` makes a symbol from identity text. It is `#[doc(hidden)]`: the checker assigns every symbol, and a consumer never builds one from a source name.
`Display` prints the source name. `Debug`, `Clone`, `Eq`, `Hash`, and `Ord` are derived.
There is no `PartialEq` with `str` or `String`, no `Deref`, no `AsRef<str>`, and no `From<String>`.

## Field list (rule 2)

Each HIR field that held identity text in a `String` now holds a `Symbol`:

| Field | Decision | Reason |
|---|---|---|
| `Callee::Func` | `Symbol` | Rule 2. |
| `ExprKind::FuncRef` | `Symbol` | Rule 2. |
| `ExprKind::Global` | `Symbol` | Rule 2. |
| `AsyncCallee::Function` | `Symbol` | Rule 2. |
| `Function.symbol` | `Symbol` | Rule 2. A method, a constructor, and a synthesized helper carry one too. |
| `Global.symbol` | `Symbol` | Rule 2. |
| `ClassDef.symbol` | `Symbol`, new | Rule 3. It is the key of the checker class table: the module symbol, the generic instance symbol, or `Error`. |
| `Callee::Method.name` | `Symbol` | It holds a method symbol. A generic method instance symbol carries the template identity (`[[identity:method:N]]`). Consumers compare it with `Function.symbol`. For a built-in member (`push`, `pop`, `slice`, `next`), the full text is the member name. |
| `AsyncCallee::Method.name` | `Symbol` | It holds a method symbol, as `Callee::Method.name` does. |
| `HostEntry.target` | `Symbol` | It holds the declaration symbol of the implementation. |
| `WorkerEntry.function` | `Symbol` | It holds the declaration symbol of the worker function. |
| `Module.synthesized_helpers` | `HashSet<Symbol>` | It holds the symbols of the synthesized helpers. |

Fields that keep `String`:

| Field | Reason |
|---|---|
| `Callee::Foreign`, `ForeignFn.name` | A C symbol name. §125 rule 3 keeps one C namespace. |
| `Function.name`, `Global.name`, `ClassDef.name` | Source names. A generic instance keeps `name<args>` without markers. |
| `EnumDef.name`, `StringAliasDef.name` | Source names. `EnumId` and `StringAliasId` are the identities. |
| `ExprKind::Local`, `Stmt::Let.name`, `Stmt::ForOf.name`, `Stmt::Try` binding, `Capture.name`, `Param.name`, `UsingBinding.name` | Local names (§124), not declaration identities. |
| `ExprKind::Field.name`, `ExprKind::EnumMember.member`, `Field.name` | Member names. |
| `PoisonedImport.module`, `PoisonedImport.names` | Specifier text and source names. |
| `HostEntry.name` | The host-visible name. |

Identity text that does not leave the checker keeps `String`: the scope items, `fn_sigs`, `class_ids`, `class_sigs`, `instance_symbols`, and `generic_fns`/`generic_classes` keys.
The checker wraps the text in a `Symbol` where it builds a HIR node.

## Consumers

The free `source_name(&str)` stays in the checker text paths: labels, messages, and the construction of `name` fields.
`warn.rs` does not call it: a foreign callee is a C name without markers, so the warning text uses the name directly. No codegen file calls it.

`codegen/src/lir` keys its function, method, and global tables by `Symbol`.
The reload declaration hash reads `full_text()`, so the hash input is the same text as before.
The narrowing path key for a global reads `full_text()` for the same reason.
Codegen error messages name the source spelling through `source_name()` (§125 rule 5).

During the migration, `Display` was absent. The build then listed every format site of a symbol, and each was read.
All were error messages. Debug formats of HIR reach only tests and panics.

## Sites the type found

The type found eight lookups in `codegen/tests/support/lir_facts.rs` that compared `Function.name` with a callee symbol.
At the contract pin `ddeaa2f` each lookup returned `None` for every module function. The call-operand and parameter-default facts then skipped that call.
The lookups now compare `Function.symbol`. The `lir` test passes with the facts applied.
A lookup miss is now a finding (malformed HIR), not a skip. See "Phase Review fixes".
`compiler/tests/support/lifetime_sites.rs` built `Callee::Func` from `Function.name`. It now uses `Function.symbol`.

## Acceptance

1. `hir::Symbol` carries two `compile_fail` doc tests, for `Callee::Func(s) if s == "name"` and for `*symbol == function.name`.
   A doc test beside them compiles the `source_name()` form and the symbol comparison.
   Stable rustdoc does not verify the error code. A control run added `PartialEq<str>`, `PartialEq<&str>`, and `PartialEq<String>` to `Symbol`. Both `compile_fail` tests then failed, so each one fails only on the comparison. The impls were removed.
2. `names::tests::a_symbol_separates_its_source_name_from_its_identity` covers `source_name`, `Display`, `full_text`, equality, and order.
   `module_global_names::class_symbols_are_unique_for_one_source_name_in_two_modules` checks two `@ValueType class RandomF32` and two `Box<f32>` instances in two modules.
   Its control asserts that the two classes of each pair have one source name.
3. See the full gate below.
4. `generated-docs/` did not change. `generate-api-reference` projects the ambient script API from `crate::ambient`, and it has no row for a Rust HIR type.
   `cargo doc` lists `hir::Symbol` and `ClassDef::symbol` from their `///` docs.

## Verification

`cargo fmt --check`, the warning-free workspace build, the release benchmark build, `generate-api-reference`, and `tools/hygiene.sh` passed.
Clippy is 3/18/13.

## Full gate at `ddeaa2f` (before the Phase Review fixes)

```text
gate full de19f5c9cf1d306cc172ad1dfbc8e78b55a335d1 dirty:51 debug 1970/0/3 release 1967/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

No `.expected` golden moved. The LIR text snapshot test passed, so the snapshot did not move (acceptance 3).

The gate ran on the tree before the contract commit was amended. `de19f5c` and `ddeaa2f` differ only in the acceptance-4 wording of the contract.

## Phase Review fixes

1. MAJOR (core principle 9). In `codegen/tests/support/lir_facts.rs`, a lookup of a declaration by `Symbol` no longer skips on a miss.
   `declared_function`, `declared_method`, and `declared_foreign` return a finding that starts with `malformed HIR`.
   The parameter-default fact, the call-operand fact, and the host-entry target lookup push that finding.
   `lir_facts/call_lookup_tests.rs` holds three violating forms:
   - A dropped operand at a module-function call gives `call operand count 2 is absent from LIR`.
   - A dropped read check in a default argument (`pick(i: i32 = table[3])`, called as `pick()`) gives `trap "IndexRead" carries 0 site(s); HIR requires 1` at the default.
   - A callee symbol that names no module function gives two `malformed HIR` findings, one per fact.
   Control: the lookup was made to miss for every module function, and each miss was made a skip. All three tests then failed. The change was reverted.
   With the change, the full `lir` test passes (56 tests), so no corpus entry has a lookup miss.
2. MINOR. The `Symbol` docs state the scope of uniqueness: a declaration symbol is unique in the program, and a member symbol is unique within its class. A built-in member's full text is the member name.
3. MINOR. `Symbol::from_full_text` is `#[doc(hidden)]`. Its doc states that `Symbol::from_full_text("main") == function.symbol` is always false for a module function. `compiler/tests/support/lifetime_sites.rs` keeps its use for a built-in member name.
4. MINOR. `warn.rs` renders a `Callee::Foreign` name directly.
5. MINOR. This file cites `ddeaa2f`.


## Full gate after the Phase Review fixes

```text
gate full ddeaa2f5873a75497bd3b2522219027658f5d7ec dirty:52 debug 1973/0/3 release 1970/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The three new tests account for the debug and release count change (1970 → 1973, 1967 → 1970).
