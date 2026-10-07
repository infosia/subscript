# §175: A counted capture stays in its block

The checker rejects a counted capture at a local outside the captured binding block.
The S009 message names the captured binding and the destination local.
The `CaptureOutlivesBlock` rejection site carries the C5 divergence.

The §118 analysis records lexical blocks during its existing HIR walk.
Its existing local equations join capture blocks and capture facts to a fixed point.
Both facts use the same clean values and flow operands.
The analysis traces rejected escape values through local equations and captured carriers.
A §118 escape boundary keeps its diagnostic without a second §175 diagnostic for that value.
A synthetic `using` scope keeps the source block.

## Pin evidence

The contract pin is `d19ed912`.
TypeScript 5.9.2 accepts all four `r396` entries and `a345`.
Each TypeScript project extends the repository configuration and includes the ambient prelude.
Each CLI run uses a temporary copy of the entry.

| Entry | Pin checker | Pin dev JIT | Pin C AOT | Pin interpreter |
|---|---|---|---|---|
| `r396-counted-capture-direct` | Accepts | Exit 1; use-after-delete | Exit 3; async resume without completion | Internal async resume without completion |
| `r396-counted-capture-copy` | Accepts | Exit 1; use-after-delete | Exit 3; async resume without completion | Internal async resume without completion |
| `r396-counted-capture-transitive` | Accepts | Exit 1; use-after-delete | Exit 3; async resume without completion | Internal async resume without completion |
| `r396-counted-capture-loop` | Accepts | Exit 1; use-after-delete | Exit 3; async resume without completion | Internal async resume without completion |
| `a345-counted-capture-block` | Accepts | Exit 0; golden output | Exit 0; golden output | Golden output |

Each `r396` entry now gives one S009 at the assigned value on line 11.
The direct and loop values start at columns 28 and 61.
The copy and transitive values start at column 47.

The three tiers and Node give this `a345` output at the pin:

```text
1 1 1
0
1
2
3 5 4
```

## Checker cost

The measurement covers 808 programs across accept, reject, warn, trap, and interop corpus arms.
The set includes the new entries in both measurements.
The checker cost pin is `c41d4d19`.
The pin measurement precedes the new measurement in the same session.
No build or test runs during either measurement.
The timer covers `check_program` calls in one debug process.
Source loading occurs before the timer starts.

| Checker | Run 1 (ms) | Run 2 (ms) | Run 3 (ms) | Best (ms) |
|---|---:|---:|---:|---:|
| Contract pin `c41d4d19` | 2103.384 | 2114.824 | 2106.539 | 2103.384 |
| New checker | 2125.833 | 2169.624 | 2174.202 | 2125.833 |

The best time increases by 22.449 ms, or 1.07 percent.
The ratio is 1.01067, below the 1.02 limit.

The shared flow operand function visits values without a temporary operand vector.
The local metadata borrows each name and tests counted types only at lambda captures.
The existing HIR walk selects equations from functions with a counted capture.
Only those equations need capture block sets and the block boundary check.
The walk retains lexical block identities because a later lambda can capture an earlier local.
Each local read joins its source set directly into the destination set.
The block boundary check uses the final equation sets from the fixed point.

The pin accepts 443 programs in this set.
The new checker accepts 439 programs.
The four new `r396` entries account for the difference.

## Verification evidence

| Check | Result |
|---|---|
| `cargo build --offline --locked --workspace --all-targets` | Pass |
| `cargo test --offline --locked -p subscript-compiler` | Pass |
| `cargo test --offline --locked -p subscript-codegen` | Pass |
| `cargo test --offline --locked -p subscript-examples --test gate` | Pass |
| `cargo fmt --check` | Pass |
| `cargo clippy --offline --locked --workspace --all-targets` | Pass; no new warnings |
| `tools/hygiene.sh` | Pass |

Every existing accept, warn, trap, and interop corpus program stays accepted.
All 17 example programs and all 14 Subscript benchmark workloads stay accepted.

The 13 §175 unit tests include an accepted control for each rejection shape.
The §154 total test checks the new site, message, divergence, and TypeScript class.
The concrete-HIR shape inventory names the shared flow operand function.
The LIR golden adds only the `a345` section, which contains 49,197 bytes.
All other LIR sections keep their exact bytes.
No existing `.expected` file changes.
The generated corpus index and language reference include the new corpus entries.

## C5 corpus additions

Add `a345-counted-capture-block` to the C5 accept list.
Add these entries to the C5 reject list:

- `r396-counted-capture-direct`
- `r396-counted-capture-copy`
- `r396-counted-capture-transitive`
- `r396-counted-capture-loop`

## Changed files

- `codegen/tests/lir-goldens/corpus.txt`
- `compiler/src/check/capture.rs`
- `compiler/src/check/capture/blocks.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/diag.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/counted_capture_block.rs`
- `corpus/accept/a345-counted-capture-block.expected`
- `corpus/accept/a345-counted-capture-block.ts`
- `corpus/reject/r396-counted-capture-copy.ts`
- `corpus/reject/r396-counted-capture-direct.ts`
- `corpus/reject/r396-counted-capture-loop.ts`
- `corpus/reject/r396-counted-capture-transitive.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/tracking/s175-counted-capture-block.md`
