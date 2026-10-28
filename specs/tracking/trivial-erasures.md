# Four forms that erase — `compiler.md` §204

Status: contract.

## Measurement at the pin `c258a296`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a390-trivial-erasures` | no error | 16 errors: S014 global `isNaN` (2) and `isFinite` (3); S100 "cannot be interpolated into a template" (8); S100 "expression form outside the decided surface" (2); S100 "statement form outside the decided surface" (1) |
| `r449-satisfies-mismatch` | TS1360 at 8:17 | S100 "expression form outside the decided surface" at 8:13 |
| `r450-template-nested-array` | no error | S100 "type `i32[][]` cannot be interpolated into a template" at 9:12 |

`node` prints the `a390` golden. `tsc` types `[] satisfies string[]`
as `never[]`: `satisfies` does not give its type to the expression.
`i32[][]` `join` gives S014 "`join` formats elements by the Q14
interpolation rules; `i32[]` elements are not interpolatable" at the
pin.
