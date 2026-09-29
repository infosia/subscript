<!-- §131 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 131. A declaration symbol is a type

*(Added 2026-09-30.)* Origin: a downstream request (subscript-typegpu,
at `2fa77ec`), with measured sites.

Problem: §125 gives each top-level declaration an identity that is not
its name. The HIR carries that identity as text inside `String`
fields: `Callee::Func`, `ExprKind::FuncRef`, `ExprKind::Global`,
`AsyncCallee::Function`, `Function.symbol`, and `Global.symbol` hold
`"[[identity:module:<hex>]]name"`, while `Function.name`,
`Global.name`, and `ClassDef.name` keep the source name. A consumer that
compares one of these strings with a source name still compiles and
finds nothing at run time. Measured downstream on the re-pin to
`2fa77ec`: the comparisons in four generator files compiled and
returned "not found"; one path could write the identity marker into
emitted shader text; and `ClassDef` has no module-qualified key, so a
program class and a library class of one source name resolved to the
library class, with the wrong layout and no diagnostic. The consumer
had to find each site by reading code (core principle 8: the form does
not carry the fact that a symbol is not a name).

### 131.1 Rules

1. The HIR has a `Symbol` type: the identity of a top-level
   declaration (§125 rule 2). Its representation is private. It has
   `source_name()`, which returns the source spelling, and an explicit
   accessor for the full text. `Display` prints the source name. It is
   `Eq`, `Hash`, `Ord`, and `Clone`. It has no `PartialEq` with `str`
   or `String`, and no `Deref` to `str`, so a comparison of a symbol
   with a source name does not compile.
2. `Callee::Func`, `ExprKind::FuncRef`, `ExprKind::Global`,
   `AsyncCallee::Function`, `Function.symbol`, and `Global.symbol` hold
   a `Symbol`. Every other HIR field that holds an identity, if any,
   holds a `Symbol` too; the round lists each one.
3. `ClassDef` gains `symbol: Symbol`, the module-qualified identity of
   the class, unique in the program, as functions and globals have.
4. The free function `source_name(&str)` stays only where a text path
   needs it (the C emitter, messages); a HIR consumer reads
   `Symbol::source_name`.
5. The language does not change: the checker's results, both tiers,
   every golden, and the LIR text snapshot stay the same.

### 131.2 Acceptance

1. A `compile_fail` doc test shows that `Callee::Func(s) if s == "name"`
   and `symbol == function.name` do not compile, beside a doc test that
   compiles the `source_name()` form.
2. Unit tests: `Symbol::source_name`, `Display`, the full-text accessor,
   and the uniqueness of `ClassDef.symbol` for two classes of one
   source name in two modules.
3. No `.expected` golden moves; the LIR text snapshot does not move.
4. `Symbol` and `ClassDef::symbol` carry `///` docs, so `cargo doc`
   lists them. *(Corrected 2026-09-30: the first text named the
   generated API reference, which lists the script-facing ambient API
   only, not Rust HIR types.)*
