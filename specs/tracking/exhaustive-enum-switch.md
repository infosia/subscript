# A switch over every enum member ends the flow — `compiler.md` §200

Status: contract.

## Measurement at the pin `409a05f5`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(TypeScript `transpileModule`, ES2022), and `target/debug/subscript`
built from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a383-exhaustive-enum-switch` | no error | S100 "not all paths return a value" ×3 |
| `t112-enum-switch-non-member-value` | no error | S100 "not all paths return a value" |

With one member missing, `tsc` gives TS2366. `node` prints the `a383`
golden: `0 4 3`, `low high`, `9 2`.

A value that is not a member reaches a switch: `Context.fromBytes<V>`
of the bytes `[9, 0, 0, 0]`, for a value class `V` with one field of
a two-member enum, gives the field value `9` (`subscript run`).
