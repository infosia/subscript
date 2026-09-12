<!-- §97 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 97. A `using` binding can be null

*(Owner decision 2026-09-09.)* Origin: the owner asked for three
decided restrictions to be reconsidered on their merits. This is the
first.

§60.1 rule 3 rejects a nullable initializer and says "narrow first,
then bind". C11 records the same as a divergence. Neither states a
type-safety problem, because there is none: disposal is guarded, and a
guard is what JavaScript already does.

The workaround the rule forces is worse than the rule admits. An outer
`if` is not a null test alone; it is a new block, and the resource
dies at that block's end rather than at the end of the scope the
programmer meant. The rule shortens a lifetime to express a check.

Measured at `de26008` on this host, and under node v24.18.0:

| Program | subscript | node |
|---|---|---|
| `using r = make(false)` returning `Res \| null` | S100 at the initializer | body runs, no disposal |
| `using r: Res \| null = null` | the same S100 | body runs, no disposal |
| `using r = maybeNoHook()`, no hook on the class | the same S100 | disposal fails at runtime |
| `using a = make(true); using b = make(false);` | S100 | `open:r open:none body dispose:r` |

One diagnostic covers three different problems today, and it names
the fix for only one of them.

### 97.1 Rule

1. `using x = e` is accepted when the binding's type is a reference
   class `R` that declares `[Symbol.dispose](): void`, or `R | null`,
   or a nullable whose class satisfies the same hook requirement.
2. A class that declares no hook stays rejected, nullable or not. The
   diagnostic names the missing hook. It no longer tells the reader
   to narrow, because narrowing is no longer the fix.
3. The binding keeps its source-visible nullable type and stays
   immutable. Member access through it needs ordinary null narrowing,
   as any other nullable local does. §97 adds no narrowing form.
4. At each exit that §60.1 rule 4 already names, a nullable binding
   disposes only when it holds a value. A null binding disposes
   nothing and changes nothing else: it does not skip the body, the
   other bindings, or the exit.
5. Order is unchanged. Reverse declaration order within a scope, the
   innermost scope first, and the return expression before the
   disposals.
6. The initializer evaluates exactly once, in declaration order,
   whether or not it yields null. An initializer with effects keeps
   them.
7. A `switch` binding tests its active flag before it reads its
   storage, and tests the stored value for null after that. A
   declaration that never executed reads no storage and disposes
   nothing.
8. Everything else in §60 stands: one evaluation, per-iteration
   disposal in a loop, a suspension is not an exit, a trap runs no
   disposal, `await using` is rejected, and a value class or a
   descriptor class that declares the hook is rejected.
9. §60.1 rule 8 stands, and it is the constraint on the
   implementation. The rewrite stays checker-complete: the guard is
   an `If` over a null comparison that the HIR already has, with the
   receiver narrowed through the established representation. No new
   HIR node, no codegen change, no runtime change. A tier-specific
   null check would break rule 8 and is forbidden.
10. `using x = null` without an annotation stays rejected, by the
   language's existing rule that a bare `null` initializer infers no
   type ("cannot infer a type from `null`; annotate the
   declaration"). `using x: R | null = null` is the accepted
   spelling. That is not §97's rule and §97 does not change it.

### 97.2 What this supersedes

§60.1 rule 3's second and third sentences retire. The first sentence
stands for the hook requirement.

C11's first listed divergence retires. JavaScript skips disposal for a
null binding and so does this language now. C11's second divergence,
that a trap runs no disposal, stands.

`corpus/reject/r131-using-nullable-init.ts` retires, and its harness
row is removed. `Divergence::UsingDeclaration` keeps its variant for
`await using`; its text drops the nullable claim.

### 97.3 Sites

- `compiler/src/check/stmt.rs` `check_bindings`: the `dispose` arm
  accepts `Type::Nullable(Class)` when the class satisfies the hook
  requirement, and splits the diagnostic so a missing hook names the
  hook.
- `compiler/src/check/mod.rs`: `UsingBinding` records whether the
  binding is nullable; `make_disposal_statements` wraps a nullable
  binding's hook call in the null guard; the switch-storage path
  applies rule 7's two tests in that order.
- `compiler/src/divergence.rs`: the `UsingDeclaration` text.
- `compiler/src/language_reference.rs` and `generated-docs/`.

### 97.4 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the four measured rows above,
recorded on this host with their exit codes.

1. One accept entry, `js-comparable: yes`, ASCII: a factory that
   returns null and one that returns a value, with a counter proving
   one evaluation each; `using x: R | null = null` with a body
   marker; a null and a live binding in one declaration, proving
   reverse order across the null; nested scopes; a natural end, an
   early return, a `break`, and a `continue`; a return expression
   whose marker precedes the disposal markers; and a narrowed member
   read through a nullable binding.
2. One accept entry for the async shape, `js-comparable: no C8`: a
   null and a live binding held across a suspension, disposed at
   completion, with an explicit `Context.collect()` while the live
   one waits. The pending disposal keeps its receiver reachable.
3. One accept entry for the `switch` shape: an entry that skips a
   declaration, then falls through into an executed one. The skipped
   declaration disposes nothing and reads no storage.
4. Rejections that stay, each with a positive control: a class with
   no hook, nullable and not; `await using`; a value class and a
   descriptor class that declare the hook; a member read through a
   nullable binding without narrowing.
5. `corpus/reject/r131-using-nullable-init.ts` is deleted and its
   harness row removed, as r104 was in §64 rule 7.
6. Unit tests in the same commit: the HIR of a nullable binding holds
   an `If` whose condition is a null comparison and whose call
   receiver carries the non-null class type; the HIR of a non-nullable
   binding holds the bare call, unchanged; a declaration that never
   executed produces no storage read.
7. *(Corrected 2026-09-09, after the round measured it.)* The
   aggregate LIR text snapshot `codegen/tests/lir-goldens/corpus.txt`
   gains a block for the new async entry, exactly as §93.3 item 8
   records for a187. `coroutine_and_measurement_lir_text_matches_goldens`
   collects every async corpus entry, so any new one adds a block by
   construction. Capture the snapshot with
   `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`, record the move under the §2
   procedure, and prove that removing the new block reproduces the
   committed snapshot byte for byte.
8. *(Added 2026-09-09, after the round measured it.)* **The execution
   fact walk must model a terminating block.**
   `codegen/tests/support/lir_facts.rs` `stops_statement_sequence`
   returns false for every `hir::Stmt::Block`, so the disposal the
   rewrite appends after a nested block that returns counts as a
   required trap site although no path reaches it. The LIR correctly
   carries one site and the walk demands two.

   A statement stops a sequence when every path through it
   terminates. Implement that for `hir::Stmt::Block`, whose own
   sequence stops, and for `hir::Stmt::If` with both arms present and
   both stopping. Report what a loop and a `switch` need, with a
   program that reaches each or a statement that none does. This
   makes the check more accurate; do not weaken it, and do not change
   the corpus to avoid the shape.

8a. *(Added 2026-09-09 by the Phase Review; item 8's report was not
   produced, and the missing half is a live defect.)* **A `switch`
   stops a sequence when it has a `default` arm and every arm stops.**
   Measured: a function whose `switch` returns from every arm, with a
   statement after it, reports two dropped facts at that statement,
   because the walk treats the `switch` as falling through. In the
   §97 shape — a `using` binding whose scope ends in an exhaustive
   `switch` — the walk demands three trap sites where the LIR carries
   two. The gate is green only because no accept entry combines the
   two; an entry that does fails
   `every_hir_execution_fact_is_carried_by_lir`.

   **A loop needs nothing**, measured: `while (true) { … }` and a
   `for` with no condition each keep the trailing statement in the
   LIR, so the walk and the LIR already agree.

   This is the second instance of the class item 8 named. The fix is
   the class: every statement kind answers the question, and the
   answer is derived from the kind's own arms rather than added one
   kind at a time. A corpus entry combining a `using` binding with an
   exhaustive `switch` lands with it.
9. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden byte-identical except the one item 7 names.
