# A class cannot declare `implements` — `compiler.md` §205

Status: landed.

## Measurement at the pin `40cf8887`

Tools: `tsc` 5.9.2 with this project's options and
`target/debug/subscript` built from the pin.

| Program | `tsc` | subscript at the pin |
|---|---|---|
| `r451-class-implements` | no error | no error |
| `class B implements A`, then `use(new B())` with `use(a: A)` | no error | S005 "nominal types are not interchangeable: the argument expects `A`, got `B`" |
| `@ValueType class U implements V`, `class G<T> implements W`, `class Z implements W, V` | — | no error |

No file in `corpus/`, `examples/`, or `docs/` declares `implements`.

## Landing

`gate full 6626fa04 dirty:16 debug 2982/0/5 release 2979/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

A mirror `declare class Q implements P` does not reach the class path,
so it gives no diagnostic (`subscript check --mirror`). bindgen does
not write the clause.
