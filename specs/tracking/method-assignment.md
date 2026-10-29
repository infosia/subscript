# An assignment to a method names the method — `compiler.md` §209

Status: landed.

## Measurement at the pin `8d4e0059`

| Program | `tsc` 5.9.2 | subscript at the pin |
|---|---|---|
| `r458-method-assignment` (`c.v = two;`) | no error | S004 "nominal types are closed: `C` has no property `v`" at 19:5 |
| `t.v = t.v;` with `t: T`, `T extends I` | no error | S004 "… `I` has no property `v`" at the target; S100 "method `v` may only be called, not read as a value" at the value |
| `C.s = two;`, `s` static | no error | S100 "static method `C.s` may only be called" |

## Landing

`gate full 884e302a dirty:17 debug 3016/0/5 release 3013/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`. Rule 2 also covers a static generic method as a write
target, which gave S018 "class `C` has no static member `s`" at the
pin.

Phase Review fix (§209.3): `gate full cf813d89 dirty:17 debug
3017/0/5 release 3014/0/5 skips 2/0 clippy 5/18/11 goldens-moved 0
exit 0`. `tsc` 5.9.2 gives TS2365 for `c.v += 1`, TS2356 for `c.v++`,
and TS2803 for `this.#v = two`; it accepts `=`, `||=`, `&&=`, and
`??=` on a method.
