# An assignment to a method names the method — `compiler.md` §209

Status: contract.

## Measurement at the pin `8d4e0059`

| Program | `tsc` 5.9.2 | subscript at the pin |
|---|---|---|
| `r458-method-assignment` (`c.v = two;`) | no error | S004 "nominal types are closed: `C` has no property `v`" at 19:5 |
| `t.v = t.v;` with `t: T`, `T extends I` | no error | S004 "… `I` has no property `v`" at the target; S100 "method `v` may only be called, not read as a value" at the value |
| `C.s = two;`, `s` static | no error | S100 "static method `C.s` may only be called" |
