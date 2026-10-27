# A constructor parameter property declares a field — `compiler.md` §198

Status: landed.

## Measurement at the pin `27093316`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(TypeScript `transpileModule`, ES2022), and `target/debug/subscript`
built from the pin.

The ES2022 output of `tsc` declares each parameter-property field
before the class fields, runs the field initializers, then assigns
each parameter property in the constructor:

```js
class P {
    x;
    y;
    label = "p";
    constructor(x, y) {
        this.x = x;
        this.y = y;
    }
}
```

`JSON.stringify(new P(2, 3))` with a later field `total = 9` prints
`{"x":2,"y":3,"label":"p","total":9}`. A field initializer that reads
a parameter property gives TS2729. A field and a parameter property
with one name give TS2300.

| Entry | `tsc` | subscript at the pin (first diagnostic) |
|---|---|---|
| `a379-constructor-parameter-properties` | no error | S100 "constructor parameter properties are not decided" |
| `a380-value-class-parameter-properties` | no error | S100, as above |
| `r442-parameter-property-read-by-field-initializer` | TS2729 | S100, as above |

`node` prints the `a379` golden. The `a380` golden is derived by
hand: the C layout of `a: i32`, `b: f64`, `tag: i32` is 24 bytes, and
the order `tag`, `a`, `b` gives 16.

## Landing

`gate full 2324c28b dirty:20 debug 2925/0/5 release 2922/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.
