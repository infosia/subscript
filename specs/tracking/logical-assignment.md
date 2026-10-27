# Logical assignment is a tested assignment — `compiler.md` §199

Status: landed.

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

## Landing

`gate full eb703f25 dirty:25 debug 2939/0/5 release 2936/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

Phase Review fix (§199.3): `gate full 8dee175e dirty:10 debug
2943/0/5 release 2940/0/5 skips 2/0 clippy 5/18/11 goldens-moved 0
exit 0`. At `8dee175e`, `a382` printed `grid false false false false`,
`elements false false true true`, `fields false false true false` on
both tiers; `node` prints the golden.
