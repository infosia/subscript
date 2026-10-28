# A static method and a local function are function values — `compiler.md` §207

Status: landed.

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

## Landing

`gate full eae8f621 dirty:25 debug 3000/0/5 release 2997/0/5 skips
2/0 clippy 5/18/11 goldens-moved 1 exit 0`. The moved golden is
`codegen/tests/lir-goldens/corpus.txt`: it grows by the `a392` section
only, and no existing line changes.

A front-end pass (`compiler/src/parse/local_function.rs`) moves a
local function that reads no binding of an enclosing function to
module scope. Its hidden name is `[[identity:local:HEX]]f`, where HEX
encodes the enclosing chain and the same-name ordinal. No source
identifier contains `[`, and an edit outside the chain does not change
the name, so a reload keeps the function identity.
