# A function type takes a nullable handle variance — `compiler.md` §235

Status: COMPLETE.

## Red at the pin `09c32780`

The debug CLI built from the code of the pin (`95d6afb6`; the amends of
the contract changed specs only): `subscript check
a422-function-type-nullable-handle-variance.ts` gave 9 errors, each
S100, then `error: 9 error(s)`:

| Line:col | Message |
|---|---|
| 49:10 | the return value expects `(C) => C \| null`, got `(C) => C` |
| 54:31 | the argument expects `(C) => C \| null`, got `(C) => C` |
| 55:37 | the argument expects `(C) => i32`, got `(C \| null) => i32` |
| 56:33 | the initializer expects `(C) => C \| null`, got `(C) => C` |
| 58:54 | the `reduce` callback expects `(C \| null, C) or (C \| null, C, i32)` => C \| null, got `(C \| null, C) => C` |
| 60:24 | the argument expects `() => string \| null`, got `() => string` |
| 61:23 | the argument expects `() => i32[] \| null`, got `() => i32[]` |
| 62:24 | the argument expects `() => Map<i32, i32> \| null`, got `() => Map<i32, i32>` |
| 63:32 | the argument expects `(C) => C \| null`, got `(C) => C` |

## After the change

`a422` checks with no errors. The development tier prints:

```
result:4
parameter:5
initializer:5
reduce:7
string:a
array:2
map:0
field:8
return:9
nullableFunction:7
arrayMap:3,6
filter:1:6
findIndex:1
reduceElement:9
mapForEach:12
push:12
mapSet:13
coalesce:11
```

The Red table is of the first version of `a422`. The lines that the
Phase Review fixes added are forms that the pin also rejects.

The ship tier, the interpreter, and `node` give the same bytes. The C
compiler gives no warning.

## Form

- `type_fits` and `function_fits` in `compiler/src/check/type_rules.rs`
  are rules 2 and 3. The arm in `assignable_through_constraints`
  applies to function types that involve no type parameter. A type
  parameter keeps the §143 composite rules.
- `callback_type_fits` in `compiler/src/check/expr/method.rs` uses
  `type_fits` for a built-in callback (rule 4).
- The JIT `convert` and the C `emit_conversion` keep the code and the
  environment of a function value (rule 5). Before this, both tiers
  required identical function types. The static `reduce` lowering gives
  the callback result the accumulator type.
- Rule 5a and 5b: `place_function_value` in `type_rules.rs` is the one
  decision. It records a `Cast` to the type of the place after the
  assignment check accepts the value. `require_expr_assignable`,
  `place_callback_value`, `??`, `??=`, `||=`, and `&&=` call it.
- Rule 5c: the static callback lowering sees through a function `Cast`
  of a lambda or a named function. The LIR verifier accepts a handle
  operand for its `| null` parameter, so no coercion is added. The C of
  a converted `forEach` callback equals the C of the identical type.
- Rule 5d: `require_assignable` rejects a pair that needs a conversion.
  `compiler/tests/type_only_assignability.rs` lists the six remaining
  type-only calls with their reasons.
- `Type::same_function_representation` limits the codegen pass-through.
- The `generic_tsc_matrix` omission count stays 9541.

## Gate

The first run failed on `composite_constraints`, `generic_tsc_matrix`,
and `apparent_type_shapes`: the arm applied to function types with a
type parameter, and two allowlist entries of `apparent_type_shapes`
were out of date. After these causes were removed, one run:

`gate quick 44d80f0c dirty:14 debug 3214/0/5 skips 2 goldens-moved 0 exit 0`

After the first Phase Review fix: one run,
`gate quick 877ccbfa dirty:25 debug 3215/0/5 skips 2 goldens-moved 0 exit 0`.

After the second fix, the run failed one test: the LIR fact derivation
counted a converted callback as a function value. After that cause was
removed, one run:
`gate quick 8f1d6e65 dirty:33 debug 3220/0/5 skips 2 goldens-moved 0 exit 0`.

After the third fix, one run:
`gate quick 09c32780 dirty:39 debug 3224/0/5 skips 2 goldens-moved 0 exit 0`.

## Phase Review

One CRITICAL, four MAJOR, and four MINOR findings, each fixed, recorded
in §235.3. Open items in §235.4. Hygiene: clean.
