<!-- §101 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 101. A disposal is not placed where control cannot arrive

*(2026-09-09.)* Origin: the Phase Review round found an accepted
program that cannot be compiled. The defect predates §97; the round
was extending §97's tests when it hit it.

```ts
class R { [Symbol.dispose](): void {} }
function run(): void {
  using r: R = new R();
  for (;;) { return; }
}
```

```
subscript: internal error: LIR construction failed:
produced invalid LIR: function 1 (`run`): use of value 0 in block 4
is not dominated by its definition
```

exit 1 from `subscript emit`, exit 2 from `subscript run`. The checker
accepts the program and the tiers never see it.

### 101.1 What the shape is

Measured at `612d170` on this host with `subscript emit`, which does
not run the program. Every row declares `using r: R = new R();` first
unless the row says otherwise.

| Loop | emit |
|---|---|
| `for (;;) { return; }` | fails |
| `for (;;) { return; }` with a statement after it | fails |
| `for (let i: i32 = 0; ; i = i + 1) { return; }` | fails |
| `for (;;) { print("x"); }` | fails |
| `{ for (;;) { print("x"); } }` | fails |
| `if (true) { for (;;) { print("x"); } }` | fails |
| `for (;;) { break; }` | passes |
| `while (true) { return; }` | passes |
| `for (;;) { print("x"); }` with no `using` | passes |

Two facts fix the shape. A `break` targeting the loop makes it pass,
and removing the `using` makes it pass. So the trigger is a scope-exit
disposal placed after a loop that control cannot leave.

`while (true)` passes where `for (;;)` fails, although control can
leave neither. *(Answered 2026-09-09 by the round, before it changed
anything.)* The checker keeps both conditions intact and removes the
trailing disposal in neither case. The difference is in
`codegen/src/lir.rs`: `lower_while` emits a conditional branch even
for a literal `true`, so `while.exit` has a false edge and the
disposal there is dominated; `lower_for` emits an unconditional
branch to the body when the condition is absent, so `for.exit` has no
predecessor and the same disposal is not.

**`while (true)` therefore passes by accident.** Its exit block is
dominated only because the CFG carries an edge execution never takes.
Both forms are wrong in the same way, and rule 2 makes both right by
not placing the disposal at all.

### 101.2 Rule

1. The checker places a scope-exit disposal only where control can
   arrive. A statement that control cannot leave takes no disposal
   after it.
2. One predicate answers "can control fall out of this statement" for
   every statement kind, in one place. A `for` with no condition and
   no `break` that targets it cannot be left. A `while` whose
   condition is the literal `true` and that no `break` targets cannot
   be left either; §101.1 shows the two forms already differ, and
   after this section they do not.
3. The predicate is derived from each kind's own parts, so a
   statement kind added later answers the question by construction
   rather than by a site someone remembers to update.
4. §60.1 rule 8 stands. The rewrite is still checker-complete and the
   HIR still holds only forms that exist today; this section removes
   statements from the rewrite's output, it adds none.
5. Every disposal that control can reach is unchanged: order, the
   return expression first, per-iteration disposal in a loop, the
   `switch` storage and active flag, and the null guard of §97.

### 101.3 The predicate is written twice, on purpose

`codegen/tests/support/lir_facts.rs` `stops_statement_sequence` asks
the same question for the execution-fact walk, and §97.4 item 8a
extended it. **It must not call this one.** CLAUDE.md core principle 9:
a check compares two facts that were derived separately, and a check
that reads the record the checked expression wrote cannot fail. The
walk derives the answer from the HIR it is handed; the checker derives
it while building that HIR. A future reviewer will see two predicates
and read duplication; this paragraph is the answer.

### 101.4 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the six failing rows of §101.1,
recorded with their exit codes.

1. One accept entry per shape that fails today and must compile: a
   conditionless `for` that returns, one that never leaves, one with
   an initializer and an update, one nested in a block, and one in an
   `if`. Each has a `using` binding in scope. The golden shows the
   disposal that a reachable exit runs, and shows no output from an
   unreachable one.
2. One accept entry for `while (true)` in the same shapes, which
   passes today and must keep its behaviour byte for byte.
3. The `break` control: a conditionless `for` with a `break` keeps
   its disposal after the loop, because control can arrive there.
4. Unit tests in the same commit. The predicate answers every
   statement kind, with the cases that kind admits: a kind that
   control always leaves has only a leavable case, a kind that
   control never leaves — `return`, `break`, `continue`, and a call
   to `unreachable()` — has only the other, and a kind that admits
   both carries both. *(Corrected 2026-09-09; this item first asked
   for both cases from every kind, which three kinds cannot give. The
   round reported the contradiction and stopped, which is the wanted
   outcome.)* Also: the HIR of a `using` scope ending in a loop that
   cannot be left holds no disposal after that loop, beside a control
   whose loop can be left and does hold one.
5. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden byte-identical. The new entries are
   synchronous, so the aggregate LIR snapshot must not move.
