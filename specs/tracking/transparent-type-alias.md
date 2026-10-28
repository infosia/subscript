# A type alias names its aliased type — `compiler.md` §202

Status: landed.

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

`tsc` 5.9.2 also rejects `type U = U | null;` with TS2456. It accepts
`type C = D[]; type D = C;`, `Map<string, M>`, `Array<R>`, `Set<S>`,
`Promise<P>`, and `(x: F) => F` cycles. §202.1 rule 4 gives the two
groups separate rejection sites.

## Landing

`gate full da3fa155 dirty:26 debug 2968/0/5 release 2965/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

A host entry with parameters of alias types (`i32`, `f64`, `boolean`,
`u8`) gives the same `subscript boundary` output, `program.h`, and
`entry.c` as the written types.
