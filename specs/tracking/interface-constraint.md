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

Phase Review fix (§208.3): `gate full 48aae770 dirty:20 debug
3012/0/5 release 3009/0/5 skips 2/0 clippy 5/18/11 goldens-moved 1
exit 0`. The moved golden is `codegen/tests/lir-goldens/corpus.txt`:
it grows by the `a395` section only. At `48aae770`, `a395` gave "class
`Double` does not implement `run` of interface `Job`" and S018 "type
`Job` has no async method `run`"; `tsc` accepts it, and `node` prints
the golden. `tsc` accepts a member of type `(x: i64) => void` for
`(x: i32) => void` and gives TS2416 for `string` against `i32`.

## Stage 2 cost, measured 2026-10-31

No production change. A C model at `-O2` on aarch64: an object with the
runtime header shape, a `noinline` member per class, 4,096 objects, and
the fastest of nine rounds. The unit is ns per call.

| Implementers | Direct call | `switch` on the class id, one class | `switch`, random classes | Function table, random classes |
|---|---|---|---|---|
| 2 | 0.86 | 1.15 | 2.20 | 4.49 |
| 4 | 0.86 | 1.43 | 3.46 | 5.06 |
| 8 | 0.86 | 1.44 | 3.66 | 5.32 |
| 32 | 0.86 | 1.46 | 7.34 | 6.05 |

The cost of a call through an interface value depends on the count of
implementers, which another declaration sets, and on the run-time class.
Core principle 16 rejects a cost of this kind. Stage 2 needs an owner
decision on that principle before a prototype.

The owner did not adopt stage 2 (2026-10-31). A value of an interface
type stays rejected (§208.1 rule 5). `T extends I` covers the shared
method set with a direct call. No downstream request names an interface
value.
