<!-- §76 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 76. Three checker facts the HIR carries once

*(Owner decision, 2026-08-30; review findings.)*

### 76.1 The assignment target is classified before lowering

**Rule 1.** `check_assign_target` classifies the AST target into one
`Place` enum — `Local`, `Global`, `Field`, `Index`, `IndexSignature`,
`Accessor`, `StaticField` — before any member lowering. `check_assign`
and `check_update` consume the enum. Neither reads a lowered
`Call { Method { "get" }, args }` to recover the target kind.

Measured before the rule: `member_on` lowered an accessor read or an
index-signature read to a `get` call, and `check_update` and
`check_assign` matched `name == "get" && args.len() == 1` at three
sites to reclassify it (core principle 9).

### 76.2 A presence test is a HIR expression

**Rule 2.** The checker emits `ExprKind::AbsenceTest { value, negated }`
for a comparison of a string-alias value against its absence marker.
The LIR lowering reads the alias's absence discriminant from the alias
table once when it lowers the node. `narrow_paths` reads the node and
needs no alias-table closure.

Measured before the rule: the producer emitted
`Binary { Eq | Ne, Int(discriminant), StringAlias }`, and `narrow_paths`
recovered the meaning by comparing the integer against the alias table
through a threaded closure (core principle 8).

### 76.3 `using` is one flag on the binding

**Rule 3.** `check_stmt` checks `Decl::Using` directly and emits
`hir::Stmt::Let { dispose: true, .. }`. Scope exit — normal fall-through,
`return`, `break`, `continue`, and a thrown trap's unwinding where the
language defines one — inserts the dispose calls in reverse binding
order from that flag, in one function. No AST rewrite to `const`, and
no binding list keyed by `Pos`.

Measured before the rule: pass 1 rewrote `using` to `const` and
recorded binding positions; pass 2 re-identified the bindings over the
checked HIR by `Pos` equality (475 lines).

`using` in a lambda body stays `S100` (§60: nested declarations are not
in the decided surface); `r172-using-in-lambda` pins it. The first
landing of this rule accepted it, measured 2026-08-30, and the
correction restored the rejection.

**Check.** No corpus golden moves. The existing `using`, accessor,
index-signature, and absence entries are the pins. A unit test builds
each `Place` variant from source and checks the classification.
