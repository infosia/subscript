# A static method and a local function are function values — `compiler.md` §207

Status: contract.

## Measurement at the pin `20534f71`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a392-static-method-and-local-function-values` | no error | 7 × S100 "nested declarations are not in the decided surface"; 6 × S100 "static method `…` may only be called" |
| `r453-local-function-reads-a-local` | no error | S100 "nested declarations are not in the decided surface" at 9:3 |

`node` prints the `a392` golden. `@ValueType` has no `node` shim, so
`a392` uses a reference class for the static factory.
