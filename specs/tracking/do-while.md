# `do…while` runs its body before its test — `compiler.md` §203

Status: contract.

## Measurement at the pin `0b4f11b0`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a388-do-while` | no error | 6 × S100 "statement form outside the decided surface"; S100 "local `n` is read before assignment"; S011 "`Item \| null` may be null here" |
| `r448-do-while-continue-skips-assignment` | TS2454 at 16:12 | S100 "statement form outside the decided surface" at 10:3, then S100 "local `n` is read before assignment" at 16:12 |

`node` prints the `a388` golden. A `while (it === null) { … }` loop
narrows `it` to non-null after the loop at the pin.

Definite assignment (§158) runs on the HIR
(`compiler/src/check/assignment_flow.rs`). A lowering of `do…while` to
a `for` loop with a first-iteration flag gives that pass a loop that
can run zero times, so `let n: i32; do { n = 3; } while (false);`
would stay rejected. §203.1 rule 5 adds a HIR form for this reason.
