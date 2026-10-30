# A generic call infers from its contextual result type — `compiler.md` §212

Status: contract.

## Measurement at the pin `b1ec9a9b`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

`a397-contextual-result-inference`: `tsc` gives no error, `node` prints
the golden, and subscript gives 13 errors: 12 × S100 "cannot infer type
parameter `T` of … : no candidate; use explicit type arguments" and
1 × S005 for `const widened: Buffer<i64> = wrap(7);`.

| Program | `tsc` |
|---|---|
| `const a = make();`, `a.items.push(1);` | no error: `T` is `unknown` |
| `two<i32>("x")` for `two<T, U>` | TS2558 "Expected 2 type arguments, but got 1" |
| `const x: Particle[] = zero(null);`, `zero<T>(f: T \| null): T[]` | TS2322 "Type 'null[]' is not assignable to type 'Particle[]'" |

The request marks its `tsc` claim as taken from documentation. The
first row of the measured table and `a397` confirm it. The third row
is a case that the request does not name: §212.1 rule 3 exists for it.

The request also names a gap in a downstream entry point. That gap is
not in this repository.
