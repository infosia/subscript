# A type alias names its aliased type — `compiler.md` §202

Status: contract.

## Measurement at the pin `f36e06d3`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a386-transparent-type-alias` | no error | 9 × S100 "type aliases are limited to a union of two or more string literals"; 1 × S100 "not all paths return a value" (the switch over the failed alias `State`) |
| `r446-type-alias-refers-to-itself` (`type Tree = Tree[];`) | no error | S100 "type aliases are limited to a union of two or more string literals" |
| `r447-type-alias-rejected-annotation` (`type Num = i32 \| f64;`) | no error | S100, as above |

`node` prints the `a386` golden. The annotation `const x: i32 | f64`
gives S011 "unions are limited to `Ref | null`" at the pin.

`tsc` rejects `type A = B; type B = A;` with TS2456 "Type alias 'A'
circularly references itself", and accepts `type L = L[];`.
