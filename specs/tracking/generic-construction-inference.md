# A generic constructor and a generic method infer their type arguments — `compiler.md` §197

Status: landed.

## Measurement at the pin `14a8fde5`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1, and
`target/debug/subscript` built from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a377-generic-construction-inference` | no error | S100 ×14: `Box` ×7, `Pair` ×2, `new Map` ×2, `new Set` ×1, method `wrap` ×2 |
| `r440-generic-constructor-conflicting-candidates` | no error | S100 "generic class `Two` requires explicit type arguments" |
| `r441-generic-constructor-no-candidate` | no error | S100 "generic class `Bag` requires explicit type arguments" |
| `r179-generic-method-without-type-args` | no error | S100 "generic method `empty` requires explicit type arguments" |
| `r186-async-generic-method-without-type-args` | no error | S100 "generic method `load` requires explicit type arguments" |

`node` prints the `a377` golden: `5 7 true`, `3 3`, `x 1`, `3 1 1`,
`8 8`.

## Landing

`gate full fc97528f dirty:20 debug 2915/0/5 release 2912/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`. The §143 matrix gives the
same verdict as the pin in each of its 39,550 cells.
