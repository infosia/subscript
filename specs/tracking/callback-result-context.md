# A callback result takes the context of the call — `compiler.md` §229

Status: COMPLETE.

## Red at the pin `71497443`

The contract commit was amended after the Phase Review to `55f3836e`.
The code of both commits equals `12e01a1b`.

The debug CLI built from the checker files of the pin, on a copy of the
entry: `subscript check a418-callback-result-takes-context.ts` gave 11
errors, each S100, then `error: 11 error(s)`. Nine read "expects
`Mode[]`, got `string[]`" or "expects `Mode`, got `string`", and two
read "cannot infer type parameter `T` of `empty`". The sites are `map`,
`map<Mode>`, a literal body, `empty()` in a body, `flatMap`, `ap`, a
nested `ap`, a nested `empty()`, a static and an instance generic
method, and a `Mode[] | null` context. `cell` and `ap<i32, Mode>` passed
at the pin.

`r485` gives S100 "expects `Mode[]`, got `string[]`" at 12:10 at the pin
and after. `tsc` 5.9.2 gives TS2322 at 12:3.

## After the change

`a418` checks with no errors. The development tier prints:

```
flip:b,a,b
flipExplicit:a,b
constant:a,a
buffers:3:0
flat:a,a
applied:a:a
nested:b:a:0
methods:b:a,a
cell:b:a
i64:2147483648
f64:0.25
flatF64:4:0.5
nestedF64:0.25
explicitI64:2147483648
nullable:b
```

The ship tier, the interpreter, and `node` give the same bytes.

## Form

- A generic function or method: `infer_arguments` in
  `compiler/src/check/inference.rs` gives an arrow argument the
  contextual candidate of `U` as its result hint, if the parameter is
  `(…) => U`.
- `map` and `flatMap`: `check_array_method` in
  `compiler/src/check/expr/method.rs` sets the hint from the context of
  the call, or from an explicit type argument.
- The hint is the slot of §186 rule 6. It gives the expression body a
  context, and the body type still decides the result, so rule 2 needs
  no added code.
- A block body with no annotation gets no hint, so rule 5 holds.

## An explicit type argument (rule 3)

`xs: i32[] = [1]`. The pin column uses the checker files of the pin.

| Form | Pin | After | `node` |
|---|---|---|---|
| ``const r = xs.map<i64>((x: i32) => 2147483647); print(`${r[0] + 1}`)`` | `-2147483648` | `2147483648` | `2147483648` |
| ``const r = xs.map<f64>((x: i32) => x); print(`${r[0] / 4}`)`` | `0` | S007 | `0.25` |

At the pin the argument had no effect, and both programs printed a value
that differs from `node`.

## Other built-in methods (rule 6)

Each form returns a value of the stated return type, with
`type Mode = "a" | "b"`. Measured after the change with the debug CLI,
and with `tsc` 5.9.2 and the libraries of the project.

| Form | Return type | subscript | `tsc` |
|---|---|---|---|
| `xs.reduce((acc: Mode, x: i32) => "b", "a")` | `Mode` | accepts | TS2769, TS2322 |
| `xs.reduceRight(…)`, the same form | `Mode` | accepts | TS2769, TS2322 |
| `ms.reduce((acc: Mode, x: Mode) => "b")` | `Mode` | S014, an initial value is required | not measured |
| `Map.groupBy(xs, (x: i32) => "a")` | `Map<Mode, i32[]>` | S014, `Mode` is not a key kind | accepts |
| `Array.from(xs, (x: i32) => "a")` | `Mode[]` | S014, no mapper overload (§105.2) | accepts |
| `p.then((v: i32) => "a")`, `p: Promise<i32>` | `Promise<Mode>` | S100 got `Promise<string>` | accepts |
| `p.catch((e: Error) => "a")`, `p: Promise<Mode>` | `Promise<Mode>` | accepts | accepts |

`then` is the one direct analogue of §229 that stays rejected. The
`reduce` row is an accepted program that `tsc` rejects (§229.4).

## Gate

`gate quick 71497443 dirty:8 debug 3183/0/5 skips 2 goldens-moved 0 exit 0`

After the Phase Review fixes, one run:

`gate quick 55f3836e dirty:9 debug 3184/0/5 skips 2 goldens-moved 0 exit 0`

## Phase Review

One MAJOR and five MINOR findings. They are recorded in §229.3. Hygiene:
clean.
