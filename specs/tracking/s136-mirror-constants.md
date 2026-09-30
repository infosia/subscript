# §136 — A mirror constant carries its value

Contract: `specs/blocks/compiler/s136-a-mirror-constant-carries-its-value.md`.

## Red at the contract pin (checker)

Test: `compiler/tests/interop_mirror.rs` `typed_mirror_constant_is_rejected_at_its_name`.
Input: `declare const K: i32;` in the mirror `mirror.d.ts`, and
``export function main(): void { print(`${K}`); }`` in `main.ts`.
The test expects one S100 at `mirror.d.ts:1:15`.

Output at the pin: the program checks clean.
The test fails at its `expect_err` with
`a typed mirror constant carries no value: Module { ... }`.
The HIR `main` body reads
`Global(Symbol("[[identity:module:6d6972726f722e642e7473]]K")), ty: I32`,
and `module.globals` is empty.
Lowering has no storage for that global.

## Implementation

- `compiler/src/check/mod.rs` `mirror_const_value`: the one predicate for rule 1.
  The declaration is `declare`, the kind is `const`, the name has no type annotation,
  and the initializer is an integer literal.
- Pass A (`collect_ambient_consts`): a declarator that fails the predicate is one S100 at the declared name.
  The message is "mirror variable `N` is outside the decided surface; the one accepted form is
  `declare const X = <integer literal>;`".
  The name binds `ScopeItem::Poisoned`, so a read, an assignment, or an operand reports nothing more.
- Pass B (the `ast::Decl::Var` arm of `resolve_mirror_signatures`): only a declarator that passes the predicate
  gets a `u64` global signature and an `ambient_int_consts` value.
  The annotated branch and its message "ambient constants require a type annotation or an integer-literal
  initializer" are removed.
- A binding pattern in a mirror stays one §107.5 diagnostic.

## Rule 3

After the change, a mirror name reaches the HIR only through the accepted form.
A read of it folds to `ExprKind::Int`.
A write (`K = 2;`, `K += 1;`, `K++;`) is S100 "cannot rebind `const` binding `K`", so the check fails before lowering.
So no checker output gives lowering an `ExprKind::Global` of a mirror name.

The lowering "unknown global" errors (`codegen/src/lir/expr.rs`, `codegen/src/lir/place.rs`) stay.
They are the general lookup failure for every HIR global symbol (module globals, static fields),
not a mirror path, and they return an error in place of a panic.

## Measured probes (CLI, this tree)

| Mirror | `main` body | Result |
|---|---|---|
| `declare const K = 1;` | `K++;` / `K += 1;` / `K = 2;` | 1 × S100 cannot rebind `const` binding `K` |
| `declare const K = 1;` | `const r: u64 = K; print(...)` | no errors |
| `declare let L: i32;` | `L = 3; print(`${L}`);` | 1 × S100 at `m2.d.ts:1:13` |
| (program source) `declare const K: i32;` | — | S100 "module-level variables require an initializer" (rule 4, unchanged) |

## Tests

- `compiler/tests/interop_mirror.rs`
  - `typed_mirror_constant_is_rejected_at_its_name` (acceptance 1).
  - `every_other_mirror_variable_form_reports_once_at_its_name` (acceptance 2):
    `const` with a type, `let`, `var`, `let` with a value, a typed `const` used as a `u64` operand,
    a `const` with neither type nor value, and a `const` with a non-integer value.
    Each is exactly one diagnostic at the declared name, with a use (a read, and an assignment for `let`/`var`).
    Control: `declare const X = 1;` in the same mirror checks clean with no HIR global.
  - `flag_set_alias_and_ambient_const_ingest`: fixture moved to `declare const SUB_FLAG_A = 1;` (acceptance 3).
- `compiler/tests/module_global_names.rs` `each_mirror_scope_kind_yields_to_the_module_scope`:
  `declare const K = 1;` with `function read(): u64 { return K; }`.
  The hidden variant is S007 "implicit numeric conversion from `i32` to `u64`" at the sibling's `K`,
  so the sibling module's own `K` still wins.
- `compiler/tests/re_exports.rs` `mirror_export_lists_stay_outside_the_surface`: fixtures moved to `declare const value = 1;`.
- `codegen/tests/module_global_names.rs`: `typed_mirror_global_lowering_errors_use_source_names` is replaced by
  `mirror_constant_values_print_as_u64_on_every_engine`.
  The mirror is `declare const K = 11;` and `declare const F_ALL = 18446744073709551615;`.
  The output is `11 18446744073709551615 true 18446744073709551604` on the interpreter, the dev JIT, and the ship C tier.
  Cost: 584 ms (debug), most of it the C compile. Rule 1a requires the value on every engine.
  The rejected forms are in `interop_mirror.rs` only; this test does not repeat them.
- `compiler/tests/module_global_names.rs` asserts the first diagnostic of each hidden row (code and `lib.ts` column).

## Rule 1a: `bind` (Phase Review fix)

Before, at the contract pin: `bind` read each constant with `clang_EvalResult_getAsLongLong`,
printed it as a signed decimal, and emitted the members of every scalar alias.
Measured with `bindgen/tests/hardening.rs` before the fix:

| Header | Output before | Output after |
|---|---|---|
| `typedef uint64_t F; static const F F_ALL = ~(uint64_t)0;` | `declare const F_ALL = -1;` | `declare const F_ALL = 18446744073709551615;` |
| `typedef uint32_t H; static const H H_ALL = ~(uint32_t)0;` | `declare const H_ALL = 4294967295;` | same |
| `typedef uint16_t S; static const S S_ALL = (S)-1;` | `declare const S_ALL = 65535;` | same |
| `typedef int32_t G; static const G G_NEG = -1;` | `type G = i32;` and `declare const G_NEG = -1;` | `bind` fails, exit 1, no mirror written |
| `typedef int64_t P; static const P P_ONE = 1;` | `type P = i64;` and `declare const P_ONE = 1;` | `bind` fails, exit 1, no mirror written |

The failure message is "bindgen: `static const` member `G_NEG` of alias `G` is outside the mirror surface:
a member needs an unsigned integer alias (u8, u16, u32, u64)".
`subscript bind --header g.h -o g.d.ts` exits 1 and writes no `g.d.ts`; the CLI needs no change.

Implementation: `Constant::flag_member_value` in `bindgen/src/clangfe.rs` masks the evaluated bits to the alias width
and returns the error for any other scalar. `bindgen/src/emit.rs` calls it for each member.
`emit.rs` is past the §5.y limit (2,257 lines at the pin). This change edits lines only, and the file is 2,256 lines after it.

Checker side: `int_literal_value` accepts a literal up to `u64::MAX` and stores its bits as `i64`,
and the fold gives `ExprKind::Int` of type `u64`.
`18446744073709551615` checks and prints on every engine with no checker change; that test is green at the pin.
`declare const N = -1;` and `declare const N = 18446744073709551616;` are rule 2 S100 (`interop_mirror.rs`).

## Acceptance 4

No committed mirror and no `.expected` golden changes.
The committed mirrors have members of `u64` aliases only, with values below `2^63`, so their bytes do not move.
`bindgen/tests/regen.rs` (byte-identical regeneration) passes.
