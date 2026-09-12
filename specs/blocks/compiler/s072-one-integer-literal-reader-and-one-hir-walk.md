<!-- §72 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 72. One integer-literal reader, and one HIR walk

*(Owner decision, 2026-08-30; review findings.)*

### 72.1 Integer literals

**Rule 1.** `parse_integer_spelling(raw, negate)` is the one reader of
an integer literal's source spelling (R26). Each consumer — a numeric
expression, a `declare const` flag, an enum member initializer — owns
its expression shape (negation, parentheses; a nested negation such as
`-(-5)` is accepted) and its range, and calls the reader for the digits.
No consumer reads the `f64` value of the literal.

**Rule 2.** An enum is a 4-byte integer (`types.rs`: `Type::Enum` is
`(4, 4)`). An enum member value is an `i32`. A member initializer outside
`i32`, or a literal that the source spelling does not represent exactly,
is `S100` at the initializer.

**Rule 3.** `as` from an enum to a sized integer converts the member
value with the width rule of that integer: sign-extend to `i64`, and
the `i32` identity. Both tiers and the interpreter lower the conversion;
the LIR carries the target width.

Measured before the rules: `const_int_of` read the `f64` value, so
`= 9007199254740993` became `…992` and `= 2147483648` wrapped to
`-2147483648`, both with no diagnostic; `Small.A as i64` failed in the
dev JIT with a Cranelift verifier error (`arg 1 has type i32, expected
i64`) — the checker accepted the cast and the lowering emitted no
extension.

**Corpus.** `a174-enum-widening-cast` (accept), `r170-enum-member-out-of-range`,
`r171-enum-member-inexact-literal` (reject, `S100`).

### 72.2 One walk over HIR

**Rule 4.** `hir::Expr` and `hir::Stmt` each expose one child iterator
(`children()`), and the exhaustive `match` over `ExprKind` and over
`Stmt` exists once, in `hir.rs`. Every pass that reads a subtree
(warnings, trap sites, layout counts, module effects, capture analysis,
async-origin flow) folds over that iterator and matches only the kinds
it acts on. A pass never writes a `_ =>` arm that drops a subtree.

Measured before the rule: 13 hand-written walks over `ExprKind` and 13
over `Stmt`. `count_async_calls_expr` (`check/layout.rs`) had no arm for
`AsyncHandleCreate`, `AsyncHandleAwait`, `AsyncHandleTransfer`;
`assigned_roots_expr` (`check/stmt.rs`) had no arm for an object
literal. Each new HIR kind had to be added at 26 sites.

**Check.** A unit test in `compiler/` builds one expression of every
`ExprKind` and one statement of every `Stmt` kind and checks that
`children()` yields every child expression the constructor received.
