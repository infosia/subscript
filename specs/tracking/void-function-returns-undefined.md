# A `void` function returns `undefined` — `compiler.md` §250

Status: IN PROGRESS.

## Red at the pin

The debug CLI built from `git archive 05a7109b` in `$TMPDIR`, run on a
`$TMPDIR` copy:

`subscript check a435-a-void-function-returns-undefined.ts` gave 11
errors, then `error: 11 error(s)`. Each is S012 "`undefined` is legal
only in `return undefined;` and in an absence comparison", at 11:10,
15:17, 23:15, 32:10, 37:12, 47:14, 60:12, 63:25, 66:13, 69:12, and
81:12: each `return undefined;`, each `return (undefined);`, the leaf
`undefined` of `return b ? undefined : side();`, and each expression
body `undefined`.

## After the change

`a435` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`f g h k run side log 1 a early al`, one per line. `g(true)`,
`early(true)`, `leaf(true)`, and `logger.log(true)` return early and
print nothing. `main` is async, so the LIR text snapshot grows by the
`a435` block only (413 lines added, none removed).

Rule 5, measured with the CLI of the pin and the CLI after the change,
on one probe per form:

| Form | Pin | After |
|---|---|---|
| `function t(): void { print("a"); return; }` | prints `a` | the same |
| `function t(): void { return side(); }` | prints `side` | the same |
| `x === undefined` with `x: Box \| undefined` | prints `1` | the same |
| `mb(false) === undefined` | prints `true` | the same |
| `const h = (): void => { return; }; h();` | prints `h` | the same |
| `return u();` with `u(): Box \| undefined` in a `void` function | S100 "a `void` function cannot return a value" | the same |
| `function* g(): Generator<i32> { return undefined; }` | S100 "generator return values are not in the decided surface" | the same |
| `const k = () => undefined;` | S012 | S012, the new text |
| `const x = undefined;` | S012 | S012, the new text |
| `function t(): i32 { return undefined; }` | S012 | S012, the new text |
| `[1].forEach((x: i32) => undefined)` | S012 | accepted; `tsc` accepts it |
| `return undefined;` in a setter, and in a constructor | S012 | S012, the new text; `tsc` rejects both |
| `return b ? undefined : side();`, `return b ? undefined : undefined;` | S012 | accepted; `tsc` accepts both |

## Form

- `compiler/src/check/stmt.rs`. `check_void_return`, the one place of a
  return operand in a function with the result `void`, takes the
  literal `undefined`, through parentheses (`is_undefined_token`), as
  `return;`. It applies §237 rule 1 first, so a setter and a
  constructor reject the literal as each other operand. Each leaf of a
  conditional that §237 splits goes through it, so a leaf `undefined`
  is `return;` too. An async function or lambda with the result
  `Promise<void>` has the frame result `void`. The expression body of a
  lambda with the result `void` goes through `check_return`, so the
  body `undefined` is `return;` too. A lambda with no result type has
  no `void` frame result, and keeps S012.
- `compiler/src/check/diagnostic_text.rs`. `UNDEFINED_PLACES` is the
  one text of the places where `undefined` is legal (rule 4). The four
  S012 sites use it: `stmt.rs` `check_return_undefined`,
  `expr/literal.rs`, and the two absence comparison sites of
  `expr/operator.rs`.
- The rejection targets, the why of the divergences
  `UndefinedEqualityPair` and `UndefinedEqualityNonMember`, the comment
  of `is_undefined_ident` (`compiler/src/check/expr.rs`), and the
  tutorial line on `undefined` take the new text.

## Open

- §250.4 item 1: a binding named `undefined` is read as the literal.

## Files

- `compiler/src/check/stmt.rs`, `compiler/src/check/expr.rs`,
  `compiler/src/check/diagnostic_text.rs`,
  `compiler/src/check/expr/literal.rs`,
  `compiler/src/check/expr/operator.rs`
- `compiler/src/check/rejection_targets.txt`,
  `compiler/src/divergence.rs`,
  `compiler/src/divergence/builtin_calls.rs`
- `compiler/tests/void_function_returns_undefined.rs` (new),
  `compiler/tests/undefined_result.rs`,
  `compiler/tests/apparent_type_shapes.rs`
- `corpus/accept/a435-a-void-function-returns-undefined.ts` and
  `.expected`
- `codegen/tests/lir-goldens/corpus.txt` (the `a435` block),
  `generated-docs/corpus-index.md`
- `docs/tutorial-typescript.md`
- `specs/blocks/compiler/s237-a-void-function-returns-a-void-operand.md`
  (§237.4 item 3 closed),
  `specs/blocks/compiler/s250-a-void-function-returns-undefined.md`
  (§250.3 and §250.4)
