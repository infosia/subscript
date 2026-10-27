# Logical assignment is a tested assignment — `compiler.md` §199

Status: contract.

## Measurement at the pin `c3903596`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(TypeScript `transpileModule`, ES2022), and `target/debug/subscript`
built from the pin.

| Entry | `tsc` | subscript at the pin (first diagnostic) |
|---|---|---|
| `a381-logical-assignment` | no error | S100 "assignment operator outside the decided surface" |
| `r444-logical-or-assignment-on-string` | no error | S100, as above |
| `r445-nullish-assignment-on-non-nullable` | no error | S100, as above |

The spelled forms are accepted at the pin: `if (x === null) { x = v; }`
narrows a local and a `this` field, and `x !== null ? x : (x = v)`
types a local and a field. An array element does not narrow.

`node` prints the `a381` golden. It counts each call of `make` and
each evaluation of the non-place receiver `pick(h)`.
