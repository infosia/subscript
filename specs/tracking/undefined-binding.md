# A binding named `undefined` is a binding — `compiler.md` §253

Status: COMPLETE.

## Red at the pin

I ran the debug CLI of `cd3afcf0` on `$TMPDIR` copies. The contract
commit changed specs only after `6f508a61`, so the code is the code of
`6f508a61`.

- `subscript check r499-a-binding-named-undefined-is-a-binding.ts` gave
  "no errors", and `subscript run` gave exit 0 with no output.
- `subscript check a437-a-binding-named-undefined-is-a-binding.ts` gave
  3 errors, then `error: 3 error(s)`:
  - 8:10 S012 "`undefined` is legal only as a result `T | undefined`,
    as a result `void`, and in an absence comparison" (`return undefined
    + 1`);
  - 12:7 S012 "`===` compares a `null` and an `undefined` absence; use
    `==`" (`undefined === null`);
  - 22:16 S012, the first text (the `for…of` binding read).

`tsc` 5.9.2 rejects `r499` with TS2322 at 9:3, 14:3, and 17:37, and
accepts `a437`.

## After the change

`r499` gives three S100 "a `void` function cannot return a value", at
9:3, 14:3, and 17:37, the positions of TS2322. `a437` checks with no
errors. The development tier, the ship tier, the interpreter, and `node`
give the same bytes: `n 42`, `absent true false`, `q 1`, `q 2`, one per
line. `main` is not async, so the LIR text snapshot does not change.

## Form

- `compiler/src/check/lookup.rs`. `is_undefined_token` is the one query
  for the token of C7: the identifier `undefined`, through parentheses,
  with no binding of that name in scope (`binds_name`). A binding is a
  local of a scope of the function, also one that a later declaration of
  the scope owns or a `switch` case declares, or a binding of the file
  or of the global scope.
- Each reader of the token uses it: the return of a function with the
  result `T | undefined` and the return of a function with the result
  `void` (`stmt.rs`), the absence comparison (`expr/operator.rs`), and
  the identifier path that rejects the token (`expr/literal.rs`
  `check_ident`, which reads `binds_name`). Each reader runs in a body
  check, with the scopes of the function at hand.
- `expr.rs` keeps `is_undefined_name`, the test of the syntax, which
  only `is_undefined_token` reads.
- `compiler/tests/apparent_type_shapes.rs`: the hash of
  `stmt.rs::check_return` changes with the text of its guard. Its shape
  tests do not change.

## Files

- `compiler/src/check/lookup.rs`, `compiler/src/check/expr.rs`,
  `compiler/src/check/stmt.rs`, `compiler/src/check/expr/operator.rs`,
  `compiler/src/check/expr/literal.rs`
- `compiler/tests/undefined_binding.rs` (new),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/apparent_type_shapes.rs`
- `corpus/reject/r499-a-binding-named-undefined-is-a-binding.ts`
- `corpus/accept/a437-a-binding-named-undefined-is-a-binding.ts` and
  `.expected`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s250-a-void-function-returns-undefined.md`
  (§250.4 item 1 closed)
