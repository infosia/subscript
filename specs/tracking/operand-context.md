# An operand takes the context of its expression — `compiler.md` §232

Status: COMPLETE.

## Contract corrections before the landing

The coding agent stopped three times, and each stop changed the contract
before any code landed:

1. `s || mk()` and `mk() || s` with `P[]` operands: `||` takes `boolean`
   operands only (`LogicalNonBooleanOperand`). A bare `k?.make()` as a
   value needs a `??` fallback (`OptionalCallValueWithoutFallback`). The
   rows moved to the left operand of `??`.
2. `empty() as Buf<P>`: with the context, the cast is `Buf<P> as Buf<P>`,
   an identity assertion that C24 rejects. Rule 2 now gives `as` no
   context, and §212.4 item 3 stays open.
3. `r490` with `return pick("true") && c`: `tsc` rejects it (TS2322), and
   the no-candidate diagnostic carries the "TypeScript accepts" block,
   which §79.1 rule 1 forbids for a `tsc: rejects` entry. `r490` uses
   `if (pick("true") && c)`, which `tsc` accepts.

## Red at the pin

The debug CLI built from the code of the pin (`0bd4204a`):
`subscript check a419-operand-takes-context.ts` gave 8 errors, each
S100 "cannot infer type parameter … no candidate": `pick` at 44:15,
48:10, 52:15, and 68:10; `K.make` at 60:13 and 64:13; `af` at 72:21 and
72:29. `r490` gave the same S100 for `pick` at 13:10.

## After the change

`a419` checks with no errors. The development tier prints:

```
or:true:true:false
orLeft:true:true:false
and:false:true:false
optional:4:0
optionalBoth:6:0
parsed:5:0
await:2:3
```

The ship tier, the interpreter, and `node` give the same bytes. `r490`
gives S100 at 13:7 with the §149 block. `tsc` accepts it.

## Form

- `mark_operand_calls` in `compiler/src/check/inference.rs` marks each
  operand of `||`, the right operand of `&&`, the left operand of `??`,
  and the final call of an optional chain.
- `operand_context`: a literal other than `null`, a template, or an
  array literal takes no context, so its type stays (rule 1).
- The left operand of `??` takes the context with `| null`, because it
  must be nullable.
- A non-call `await` operand takes `Promise<C>`.
- `as` is unchanged. A unit test pins `id(1) as i64`, `b as Buf<P>`, and
  `empty() as Buf<P>` at their pin results.
- `a419` has an async entry, so the LIR text snapshot grows by its block
  only.

## Gate

`gate quick bcb2eda2 dirty:12 debug 3201/0/5 skips 2 goldens-moved 1 exit 0`

The moved golden is the LIR text snapshot, which grows by the `a419`
block. The Phase Review fixes after this run changed tests, the `a419`
header, and a doc comment only. Their targeted tests pass:
`contextual_result_inference` 40, `corpus_accept` 13, and the generated
references.

## Phase Review

Four MINOR findings, recorded in §232.3. Each test of the fixes fails
when the code it pins is removed. Hygiene: clean.
