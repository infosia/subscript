# A private name is a private member — `compiler.md` §206

Status: landed.

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

## Landing

`gate full 12bb76f4 dirty:27 debug 2988/0/5 release 2985/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

A front-end pass (`compiler/src/parse/private_name.rs`) makes a `#x`
member a `private` member named `#x`. Emitted C names a struct member
`d<field id>`, so `#x` and `x` get two C names, and no source name
reaches a program C member name. `r452` reports at 13:14, the column
of the member name, as for a `private` member.
