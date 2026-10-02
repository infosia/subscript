# §145: finished generator reference values

The contract is `compiler.md` §145 at `4c6329d4`. Collision C23 names the runtime divergence.

## Red

A CLI built from `4c6329d4` measured both new trap entries before the change.
`t75` returned exit 2 with `program terminated abnormally (dev-JIT child signal 11)` and empty stdout.
`t76` returned exit 0 with `1\n`. Neither entry raised `generator-done-value`.
Stock `tsc` accepted both entries. Node raised `TypeError: Cannot read properties of undefined` at `b.v` for t75.
Node printed `1\n` for t76.

## Change

`Type::traps_on_generator_done_value` requires a non-nullable handle type other than `Str`.
Value classes, scalar types, strings, dates, and enums do not satisfy the predicate.
The HIR field-read site uses this predicate. The LIR verifier independently derives the required guard from the operand type.
Both tiers and the interpreter consume the same LIR site before the value load.
The runtime code is 32, with rule name `generator-done-value`.
Iterator-result field patterns use the same field-read path and report the bound name's position.
The C emitter declares a shared pointer iterator-result type once when nullable and non-nullable generators share one module.

## Measurements

| Entry | Node stdout | dev stdout | ship stdout | interpreter stdout |
|---|---|---|---|---|
| t75 | TypeError at later field use | empty; trap at 16:20 | empty; trap at 16:20 | empty; trap at 16:20 |
| t76 | `1\n` | empty; trap at 16:20 | empty; trap at 16:20 | empty; trap at 16:20 |
| a310 | `true\ntrue\n7\n7\n7\n7\n` | same | same | same |
| a311 | `undefined\nundefined\n` | `0\n0\n` | `0\n0\n` | `0\n0\n` |

A separate field-pattern probe trapped at 7:18 in all three execution forms.
The scalar entry cites C8 and declares itself not JS-comparable.
Ship CLI probes used source copies, a runtime archive copy, and an include copy under the temporary directory.
The capture with `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1` added only the a310 and a311 sections.
No existing `.expected` file changed. The header and reference documents came from their generators.

## Verification

`cargo test --offline --locked` passed for the compiler, codegen, and runtime crates.
The dev/ship trap sweep and accept sweep passed, including all new entries.
The interpreter trap sweep and the new three-form control tests passed.
The JS corpus target passed all 11 tests. Stock `tsc -p tsconfig.json` passed.
`cargo fmt --check` and `git diff --check` passed.
A second generator run produced byte-identical runtime headers and reference documents.
The Phase Review and its collision-index follow-up reported no findings. `tools/hygiene.sh` passed.
No Rust file that received new lines exceeded 2,000 lines. No commit was made.

## Landing gate

`goldens-moved 1` is the LIR text snapshot: `a310`, `a311`, `t75`, and
`t76` are generator entries.

```text
gate full 09de36d6915f5ab4457fc0ab7acfd351dd7a706d dirty:39 debug 2190/0/3 release 2187/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```
