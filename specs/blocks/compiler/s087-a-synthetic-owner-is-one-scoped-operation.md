<!-- §87 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 87. A synthetic owner is one scoped operation

*(Owner decision 2026-09-05.)* Origin: the development-cost review of
2026-09-05 (`specs/tracking/development-cost-review-2026-09-05.md`,
finding 1).

Measured at `3677d1f`. §82.10 names seven owner kinds and states a
total boundary check. The code exposes the owner as four operations:
`FnCtx::enter_synthetic_owner`, `push_synthetic_prefix`,
`drain_synthetic_prefix`, and `Checker::finish_synthetic_owner`
(`compiler/src/check/mod.rs` lines 470–500, 1354). Thirteen sites
call `enter_synthetic_owner`; each one drains and finishes by hand
(`stmt.rs` 424, 873, 883, 904, 920, 1404; `mod.rs` 4046, 4107, 4604,
4669, 4712, 4750; `expr.rs` 8205). Seven of them route through
`close_synthetic_expression` (`expr.rs` 2382), which rejects a
non-empty prefix. The return type of `check_expr` does not state that
a prefix can exist. R39 review round 2 (three owners drained into the
wrong list) and round 3 (the `switch` case owner missing) were two
instances of one protocol with no type to hold it.

### 87.1 Rule

1. **One operation.** `FnCtx` exposes one owner operation:
   `with_synthetic_owner(kind, |fx| body)`. It enters the owner, runs
   the body, drains the prefix, and leaves the owner on every return
   path of the body, including an early return. The four operations
   of today become private to `FnCtx`, or are deleted.
2. **The kind is a value.** `SyntheticOwnerKind` is an enum with one
   variant per owner of §82.10 rule 1: `Statement`, `Declarator`,
   `ForInit`, `ForCond`, `ForUpdate`, `ArrowBody`, `Initializer`,
   `SwitchCase`. *(`Declarator` added 2026-09-05, forced, after the
   §87 review: a `let`/`const`/`using` with two or more declarators
   had the statement as its owner, so the first round placed every
   declarator's prefix before the first binding —
   `let a: Box = new Box(2), b: i32 = (pick(a) ?? fb).v;` put the
   synthetic `Let` before `a`'s `Let`. The committed checker drained
   per declarator; the owner list did not say so. Measured after
   the review: the runtime order is unchanged (87.3 item 1), so
   this is a placement rule, not a semantics defect. A declarator of a declaration with two or more
   declarators is its own owner, and its prefix goes before that
   declarator's `Let`.)*
   The operation returns the body's result and the drained prefix as
   `SyntheticPrefix`, a value that the caller places by the kind's
   rule of §82.10 rule 2. For `Initializer` and `SwitchCase` the
   operation itself reports S100 with the §82.3 block when the prefix
   is not empty, and returns an empty prefix; a caller of those two
   kinds cannot place a prefix, because the value it receives is
   always empty.
3. **The drain is the boundary.** *(Revised 2026-09-05, forced, after
   the §87 review: in the scoped form the operation drains the
   collection and pops the stack on every return path, so the check
   of §82.10 rule 3 read a collection the same operation had just
   emptied and had no input that could fire it — core principle 9.)*
   The operation is the only code that pushes or pops the owner
   stack and the only code that drains a collection; nothing outside
   it can leave a prefix behind. The S100 "internal: synthetic
   prefix escaped its owner" report and its comparison are deleted.
   The witness of §82.10 rule 3 (the `switch` case shape) is a
   matrix cell of 87.3 item 1.
4. **The prefix has one type.** `SyntheticPrefix` wraps
   `Vec<hir::Stmt>`. A `Vec<hir::Stmt>` that is a prefix is not passed
   bare; the type names the fact.
5. **No semantics move.** Every diagnostic, position, evaluation
   order, and count of §82.1, §82.3, and §82.10 is unchanged. Every
   pre-existing golden and `.expected` stays byte-identical.

### 87.2 Sites

- `compiler/src/check/mod.rs`: `FnCtx`, `SyntheticOwner` (private),
  `SyntheticOwnerKind`, `SyntheticPrefix`, `with_synthetic_owner`;
  the six `enter_synthetic_owner` sites and
  `finish_synthetic_owner`.
- `compiler/src/check/stmt.rs`: the six sites.
- `compiler/src/check/expr.rs`: the arrow-body site and
  `close_synthetic_expression`.
- `compiler/tests/synthetic_prefix.rs`: the matrix of 87.3.

### 87.3 Corpus and gate (pre-registered exit criteria)

1. `compiler/tests/synthetic_prefix.rs` gains one table-driven test
   over the matrix owner kind × receiver, with the eight owner kinds
   and two receivers (`(maybe() ?? fb).v` and `maybe()?.v ?? 0`).
   The `Declarator` cells use a declaration with two declarators
   where the second's initializer reads the first (`let a = …,
   b = (pick(a) ?? fb).v;`): the expected HIR has the first `Let`,
   then the synthetic `Let`, then the second `Let`. That cell is
   the Red for the `Declarator` owner: on the first §87 tree the
   synthetic `Let` precedes the first `Let`. *(Corrected 2026-09-05,
   measured: the synthetic `Let` of §82.3 carries a `Null`
   initializer and the receiver call stays inside the expression,
   so the misplacement has no runtime effect — `new Box 2` prints
   before `pick 2` on both tiers and under `node` on the first §87
   tree as well. No corpus entry can be red for it (core principle
   10), so none is added; the rule is a placement rule, and the
   HIR is its witness.)* For
   each cell the expected value is **hand-written**: the diagnostic
   code and position for `Initializer` and `SwitchCase`, and for the
   other five the number of synthetic locals in the lowered HIR and
   the statement index where the prefix lands (statement before, loop
   head, step, arrow body head). The test reads the HIR, not the
   checker's owner stack.
2. The same file keeps a positive control: a program whose prefix
   escapes cannot be written from outside, so the control is the
   `SwitchCase` cell, which reports S100 with the §82.3 block, and a
   unit test in `mod.rs` that calls `with_synthetic_owner` with a
   body that pushes one statement and returns early through `?`, and
   asserts the prefix comes back and the owner stack is at its
   entry depth.
3. a176, a177, and every corpus entry: goldens unchanged, both tiers
   and the interpreter, `tools/gate.sh full` verdict `goldens-moved 0`.
4. `grep -c enter_synthetic_owner compiler/src` outside `FnCtx` is 0.
