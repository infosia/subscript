# A private name is a private member — `compiler.md` §206

Status: contract.

## Measurement at the pin `0ac9c22c`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a391-private-names` | no error | 8 × S100 "class member form outside the decided surface"; 11 × S100 "private names are not in the decided surface" |
| `r452-private-name-outside-its-class` | TS18013 at 13:14 | S100 "class member form outside the decided surface" at 8:3, then S100 "private names are not in the decided surface" at 13:12 |

`node` prints the `a391` golden. `node` omits a `#x` field in
`JSON.stringify` (`{"b":2}` for `class C { #a = 1; b = 2 }`).
`tsc` rejects `o?.#x` with TS18030.

At the pin, `JSON.parse<C>` needs a key for every field, also for a
`private` field with an initializer: a missing key traps "document
does not match P at $.tag: expected string, found no field".
