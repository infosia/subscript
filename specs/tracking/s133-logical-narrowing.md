# §133 — A logical operand sees the narrowing of the operand before it

Contract: `specs/blocks/compiler/s133-a-logical-operand-sees-the-narrowing-of-the-operand-before-it.md`.

## Rule 2a (§5.y)

No file passes 2,000 lines. No split is necessary.
`compiler/src/check/expr/operator.rs` is 1,319 lines, `compiler/src/check/stmt.rs` is 1,513 lines.

## Red (acceptance 1)

Entry: `corpus/accept/a298-logical-operand-narrowing.ts`. Binary: `subscript check`, built from `fbb31d5` before the change.
Result: exit 1, 20 errors, each S011. Positions (line:column):

```
29:22 30:15 36:37 36:54 41:37 41:54 46:22 49:10 53:21 53:37 53:55 54:14 60:40 75:21 76:25 79:44 81:42 82:45 85:42 85:58
```

The entry above is the first version. The Phase Review extended it with three shapes. The same binary on the extended entry: exit 1, 25 errors: 24 S011 and one S010. Positions:

```
29:22 30:15 36:37 36:54 41:37 41:54 46:22 49:10 53:21 53:37 53:55 54:14 60:40 71:51 80:12 80:18 85:42 85:48 99:21 100:25 103:44 105:42 106:45 109:42 109:58
```

- 71:51, S010: `e instanceof RangeError && e.message.length > 0` in a `catch` clause. The catch binding is used outside `instanceof` and `throw`.
- 80:12 and 80:18, S011: the `else` branch of `if (a === null || b === null)` reads `a.v + b.v` (rule 2).
- 85:42 and 85:48, S011: the ternary form `a === null || b === null ? -1 : a.v + b.v` (rule 2).
- The other positions are the lines of the first version, moved by the three new functions (24 lines).

Each measured shape is in the list: a parameter (29, 36, 41), a local (75, 81, 82, 85), `&&` (29, 36, 53, 60, 75, 79), `||` (41, 46, 81, 82, 85), a nested `&&` chain (53), a shared field path (60, `h.c` of a reference class), a local path (79, `x.v` of a local `Leaf | null`), and value positions (`const b: boolean = ...`).
Lines 30, 49, 54, and 76 follow from the operand error: the right operand has the type `<error>`, so the condition narrows nothing for the body.

The measurements below apply to the extended entry.
`tsc` 5.9.2 with `prelude/lang.d.ts`, the `corpus/interop` ambient files, and the options of `compiler/tests/tsc_corpus.rs`: exit 0, no diagnostics.
`node` through `corpus/node/run-js-corpus.cjs`: status `ok`, output byte-identical to the golden. The header states `tsc: accepts; js-comparable: yes`.
After the change, `subscript run` (dev JIT) output is byte-identical to the `node` output.
The golden is the dev JIT output. The ship C executable from `subscript build --source` gives the same bytes.

## Mechanism (rules 1 to 4)

`Checker::check_logical_right` (`compiler/src/check/expr/operator.rs`) checks the right operand.
It uses the scope mechanism of the conditional expression (`check_cond`):

1. It derives the facts of the left operand with `Checker::narrowing_paths`: the true facts for `&&`, the false facts for `||`.
2. It checks the right operand with `fx.narrowed` = the facts before the operator plus those facts.
3. It keeps each kill inside the operand: a fact from before the operator that the operand ended stays ended (§124 kills go through the same `fx.narrowed` state as a conditional arm).
4. It restores the facts from before the operator and calls `finish_narrowing_join` (rule 4).

A narrowed shared read inside the right operand has the narrowed type, so the lowering gives it its `NarrowNonNull` site as in an `if` body (§124 rule 3d).

Rule 2: `Checker::narrowing_paths` (`compiler/src/check/narrowing.rs`) derives the false facts of `a || b` as the union of the false facts of both operands.
It removes a fact of `a` that a kill in `b` ends, for `&&` and `||` alike.
It is the only function that splits `&&` and `||`. `narrow_paths` (`compiler/src/check/stmt.rs`) reads a leaf condition only.

## Tests (acceptance 2)

`compiler/tests/logical_narrowing.rs`. Each test asserts the exact diagnostic list (code, line, column, message, divergence).

- `the_right_operand_sees_the_left_facts`: `&&`, `||`, a shared path, a local, and a chain check clean.
- `a_call_in_the_right_operand_ends_the_shared_narrowing`: `h.c !== null && clear(h) && h.c.v > 0` and the `||` form give one S011 with `SharedLocationNarrowing`. Controls: the read before the call, and a local across the call, check clean.
- `an_await_in_the_right_operand_ends_the_shared_narrowing`: one S011 with `SharedLocationNarrowing`.
- `the_facts_end_with_the_operand`: `const x = a !== null && true; x ? a.v : 0` and the `||` form give one S011 without a divergence. Control: the same conditions in `if` statements check clean.

- `a_statement_else_branch_reads_the_false_facts_of_logical_or`: the `else` branch of `if (a === null || b === null)` reads `a.v + b.v` clean. Control: the `&&` form gives two S011.
- `a_call_in_the_right_operand_ends_the_statement_facts`: `if (h.c === null || !clear(h)) { ... } else { h.c.v }` gives one S011 with `SharedLocationNarrowing`, and the `&&` form in the `if` body gives the same. Control: `h.c === null || false` checks clean.

Cost: one extra integration test binary, `logical_narrowing` (debug, 12.6 MB). Debug measurement on arm64 macOS:
- Compile and link after the binary is removed: 0.27 s (`cargo test --no-run`, the library already built).
- First run: 0.50 s (the first-launch wait of a new executable). Later runs: below 0.01 s. Six tests, 0.01 s inside the harness.

## Observation, not measured

`Checker::check_logical_right` (`compiler/src/check/expr/operator.rs`, line 1139) calls `narrowing_paths` on the whole left operand.
In a left-associative chain of n operands, each operator derives the facts of its left subtree again. The work is O(n²) in the chain length.
A statement condition adds one more pass over the whole chain. No chain in the corpus has a measured cost.

## Goldens (acceptance 3)

No existing `.expected` golden moves. The new entry is neither async nor a generator, so the LIR text snapshot does not select it.
`generated-docs/corpus-index.md` gains the a298 row.

## Gate

`tools/gate.sh full`, one run, report `target/gate/20260930T003432Z-full.md`:

```
gate full fbb31d5090354508842d8c7e8be7cee63938dbc6 dirty:8 debug 1988/0/3 release 1985/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

After the Phase Review fixes, one run, report `target/gate/20260930T010445Z-full.md`:

```
gate full fbb31d5090354508842d8c7e8be7cee63938dbc6 dirty:8 debug 1989/0/3 release 1986/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The test count is one higher: the unit test of `narrow_paths` left, and two tests in `compiler/tests/logical_narrowing.rs` came in.
