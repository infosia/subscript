# An interface is a constraint — `compiler.md` §208

Status: landed.

## Facts that the design reads

- A generic function takes a class constraint at the pin:
  `function total<T extends Sq>(xs: T[])` checks, and a call of
  `x.area()` on an unconstrained `T` gives S018 "`T` has no method
  `area`".
- Each generic instance is a separate function (§140, §149), so a
  call through a constraint is a direct call.
- A reference object has a 16-byte header with a class id
  (`runtime/src/context.rs`, `HEADER_SIZE`, `header_class_id`), and the
  counted-owner release reads it (`runtime/src/context/counted.rs`). A
  stage 2 interface value can use it; this section does not.

## Measurement at the pin `3e9bd6c7`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a394-interface-constraint` | no error | 3 × S100 "declaration form outside the decided surface"; 3 × the §205 `implements` text |
| `r455-interface-value-type` | no error | S100 "declaration form outside the decided surface", then the §205 text |
| `r456-class-does-not-implement` | TS2420 at 11:7 | as `r455` |
| `r457-constraint-needs-implements` | no error | S100 "declaration form outside the decided surface" |

`node` prints the `a394` golden.

## Landing

`gate full 5a186b31 dirty:42 debug 3008/0/5 release 3005/0/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 0`.

`Type::Interface` is a checker-only constraint type; the HIR never
carries it, because each instance substitutes the class. `a394` is the
first corpus entry with two instances of one generic function that
contains a `for…of`. The LIR fact model in
`codegen/tests/support/lir_facts/iteration.rs` keyed the expected
cursors by source position and expected one cursor where the LIR has
two; it now counts each HIR occurrence. The generic tsc matrix adds
629 cells for the interface constraint kind (40,174 cells in 30.46 s).
