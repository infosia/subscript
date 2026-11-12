# A conditional in a statement place takes `void` leaves — `compiler.md` §258

Status: COMPLETE.

## Red at the pin

The contract pin is `4895ab15`. Its code is the code of `908c35b7`. The
debug CLI of that code, run on a `$TMPDIR` copy:

`subscript check a442-a-conditional-in-a-statement-place-takes-void-leaves.ts`
gave 13 errors, each at a `void` leaf:

```
error[S100]: a `void` expression is only allowed as an expression statement
 --> x.ts:29:8
 --> x.ts:30:8
 --> x.ts:31:7
 --> x.ts:32:11
 --> x.ts:32:26
 --> x.ts:32:33
 --> x.ts:33:12
 --> x.ts:33:19
 --> x.ts:41:20
 --> x.ts:43:20
 --> x.ts:44:33
 --> x.ts:44:40
 --> x.ts:47:28
error: 13 error(s)
```

Each `-->` line follows the same S100 message. `tsc` 5.9.2 accepts
`a442`. `node` prints the `.expected` output.

## The change

`compiler/src/check/expr/void_leaves.rs` holds the place of the leaves
of the next conditional, `&&`, or `||` (`VoidLeafPlace`):

- An expression statement sets `Statement` for its root. The expression
  body of a lambda with no result annotation sets `VoidResult`. A
  contextual `void` lambda body reaches the expression statement path
  (§237 rule 4).
- `check_cond` and the `&&`/`||` arm of `check_bin` take the place, and
  check each leaf with its `void` type kept. A leaf that is a
  conditional (or `&&`/`||` in a statement place) takes the same place.
- In `Statement`, a conditional with a `void` leaf, and `&&`/`||` with
  a `void` right operand, have the type `void`.
- In `VoidResult`, a conditional with two `void` leaves is `void`. A
  `void` leaf beside a value leaf gives the S100 of the pin, at the
  `void` leaf.
- `void_leaf_statements` turns a `void` conditional into an `if`,
  `a && b` into `if (a) b;`, and `a || b` into `if (a) {} else b;`.
  The order of evaluation and the short circuit stay.

A program that the pin accepts has no `void` leaf in these places, so
its HIR does not change. A `return` operand keeps the §237 path.

## Gate

The first quick gate failed:

`gate quick 4895ab1575126d14d621c863c60859960f760167 dirty:13 debug 3334/1/5 skips 2 goldens-moved 0 exit 1`

`apparent_type_shapes` found a raw type test in `void_leaf_statements`:
`expr.ty [!=Type::Void]`. The function is now a `Checker` method that
reads the apparent type.

## Measurements

- `compiler/tests/void_leaf_statement.rs`: 3 tests, 0.03 s. With the
  place never set, the test of the first table fails. The tests of
  rules 3 and 4 pass in that state too, so their diagnostics are the
  diagnostics of the pin.
- `a442` gives the same output on the development tier, the ship tier,
  and `node`.
- `main` of `a442` is not async, so the LIR text snapshot does not
  change.
