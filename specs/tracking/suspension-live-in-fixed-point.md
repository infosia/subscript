# Suspension live-in SSA repair

The contract was introduced as `431d2307` and amended as `96700297` (the current source pin).
The construction uses definitions at the original site and suspension successors,
plain liveness, iterated dominance frontiers, and a dominator-tree rename.
It has no version fixed-point loop. Unreachable predecessors are filtered before
computing dominators (compiler.md §273 rule 6).

## Cause (compiler.md §273 rule 1)

`codegen/src/lir/liveness.rs::thread_suspension_live_ins` does not reach a fixed point.
The version analysis propagates an origin through blocks where that origin is not live.
When predecessor versions differ at such a block, it selects the smallest value ID.
This selection can alternate around a cycle as predecessor versions change.
The loop sets `changed` on each alternation and never exits.

A temporary probe stopped the analysis after 100 rounds and printed its state.
In the break form, origin `%30` is the generator handle from `gen()` in `b8`.
The suspend successor defines `%45`. The generator loop header receives merge parameter `%48`.
At rounds 98 and 100, `incoming[b5]` is `%30` and `incoming[b7]` is `%48`.
At rounds 99 and 101, `incoming[b5]` is `%48` and `incoming[b7]` is `%30`.
These array-loop blocks precede the generator definition. They need no version of this origin.
The suspension analysis writes these alternating analysis states; no runtime frame exists at this point.

## LIR comparison

The probes used `subscript_compiler::lir_text::print_module`, the printer that the LIR tests use.
The pre-analysis break form defines the generator handle `%30` in array exit block `b8`.
It contains `if.then`, `if.else`, and `if.join` blocks inside the array loop.
The no-conditional control defines generator handle `%26` in `b8` and contains no such blocks.
The control completes suspension analysis and prints `end 2`.
The break form does not produce a final LIR module because suspension analysis never returns.
An empty `if (j > 0) {}` also fails, without a break or continue.
The current contract includes the empty-if form.

## Snapshot audit (rule 5)

| Entry whose snapshot block changes | Kind of change permitted by rule 5 |
|---|---|
| a149-suspension-state | Value renumbering |
| a341-reference-holder-release | Value renumbering |
| a439-an-operand-hold-is-a-release-owner | Value renumbering and four removed trivial merges in main b76 |
| a445-the-suspension-live-in-analysis-reaches-a-fixed-point | New acceptance entry |

No other existing entry changes. In particular a324, a338, a359, and a423 equal the pin.
The four live origins in a439 are %19, %20, %32, and %81.
Both predecessors of b76 pass the same version for each origin, so rule 5
permits these removals. No merge of two distinct reaching versions is removed.

| Origin | Removed parameter | Pin b72 argument | Pin b73 argument | Draft version from b72 | Draft version from b73 |
|---|---|---|---|---|---|
| %19 | %330 | %329 | %329 | %329 | %329 |
| %20 | %345 | %344 | %344 | %343 | %343 |
| %32 | %360 | %359 | %359 | %357 | %357 |
| %81 | %374 | %373 | %373 | %370 | %370 |


The control test asserts the pin's main block structure, parameter counts,
and suspension and merge parameter origins. It does not compare a whole module.
The unreachable-predecessor test asserts dominance and use renaming after a suspend.
The conditional analysis test runs on a thread with a 10-second receive timeout.
The corpus differential gate supplies tier comparisons; duplicate tests were removed.

## Validation

Results for this revision are recorded below. No gate script, commit, or push ran.

| Check | Result | Wall time |
|---|---|---:|
| Red: full a445 timeout test against unchanged production code at 96700297 | Expected failure: receive timeout at 10.01 s; test process exited with 101 | 14.097 s |
| Focused unit tests | 3 passed (full a445 plus conditional forms, control, unreachable predecessor) | 0.32 s |
| LIR tests, release model and verifier | 67 passed | 19.57 s |
| cargo fmt --check | Passed | 3.85 s |
| cargo clippy -p subscript-codegen --all-targets --offline | Passed; existing warnings, none in changed files | 9.59 s |

The pin run restored the current source in a `finally` block. Its worker ended
with the test process; no probe remains. The snapshot audit compares each entry
against the pin and confirms the four entries listed above.

Final codegen unit suite: **353 passed**, 0 failed; wall time **49.94 s**.
