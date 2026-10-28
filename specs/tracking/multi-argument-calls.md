# `push`, `Math.max`, `Math.min`, and `Array.of` take any argument count — `compiler.md` §201

Status: landed.

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

## Landing

`gate full 1abd9cb1 dirty:46 debug 2961/0/5 release 2958/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

A value-position `push` with two or more arguments lowers to one HIR
`Seq` (§201.1 rule 4a). Each synthetic local is declared unassigned in
the owner prefix, so the LIR has one `Zero()` per local and no branch
or extra call. A literal, an enum member, a function reference, or a
lambda argument gets no synthetic local: a lambda captures `const`
locals only (C5), so its value does not depend on the order.
