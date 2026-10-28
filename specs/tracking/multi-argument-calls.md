# `push`, `Math.max`, `Math.min`, and `Array.of` take any argument count — `compiler.md` §201

Status: contract.

## Measurement at the pin `43526876`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a384-multi-argument-calls` | no error | 18 errors: S100 "`push` expects 1 argument(s)" (5), S014 "takes exactly 2 f64 argument(s)" (5), S014 "Variadic parameters are outside the language" (6), S014 `Array.of` (2) |
| `r16-math-variadic-max` (`Math.hypot(1, 2, 3)`) | no error | S014 "`hypot` is rejected: Variadic parameters are outside the language. (Q19)" |

`node` prints the `a384` golden. `xs = [1]; xs.push(xs.length, xs.length)`
gives `[1, 1, 1]`: every argument is evaluated before the first append.

`Math.hypot(a, b, c)` and `Math.hypot(Math.hypot(a, b), c)` differ in
80,198 of 200,000 random triples with each value in `[0, 1000)` under
`node`. `Math.max()` is `-Infinity`, `Math.min()` is `Infinity`,
`Math.max(-0)` is `-0`, `Math.max(1, NaN, 3)` is `NaN`, and
`Math.max(-0, 0, -0)` is `+0`.
