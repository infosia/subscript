<!-- §278 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 278. The golden sweep runs each entry once

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P1, "remove focused golden tests already covered by
the complete sweep"). Iteration speed is a goal, and a gate's cost is
part of its design (core principle 15).

Problem, read in the code and measured. `codegen/tests/golden.rs` has
the sweep `jit_ship_c_aot_and_golden_agree_byte_for_byte`, which runs
each accept entry with a golden on the dev tier and the ship tier, and
compares both outputs with the golden. The file also has 33 focused
tests. They run 44 corpus entries again on both tiers, and 23 of them
run in their own process (§190.1 rule 3). The sweep already compares
each of these entries. In the full gate record of 2026-11-13 at
`8a36c8c1`, the suite took 42.88 s in debug and 27.28 s in release.

### 278.1 Rules

1. A focused test of `codegen/tests/golden.rs` is removed if each of
   its observations is one of these: the dev output equals the ship
   output, the dev output equals the golden, the ship output equals
   the golden. The sweep compares the same three facts for each entry
   with a golden.
2. A focused test with another observation stays. It keeps only that
   observation, and it does not run an entry on a tier only to repeat
   rule 1's facts. The tracking note lists each focused test that
   stays, with its observation.
3. A focused test that names an entry with no golden, or an entry that
   the sweep skips on the reference host, is not removed. The tracking
   note lists each such case. (At `8a36c8c1`, each accept entry has a
   golden, and `compiler/tests/js_corpus.rs` reports a missing one.)
4. When a dev run of the sweep ends with an abnormal termination, the
   failure line holds the stdout of the run before the termination.
   A removed focused test (`a149`) printed that text.
5. The host hooks, the file provider, the entry arguments, and the
   platform exclusions of the sweep do not change.
6. The newline test (`every_corpus_entry_with_a_golden_ends_in_a_newline`)
   stays. It reads the goldens only.

### 278.2 Acceptance

1. The tracking note lists each removed test with its entries, and
   each kept test with its observation.
2. A control for rule 4: a unit of the sweep's failure line, given an
   abnormal termination with stdout `x\n`, holds `x`.
3. `cargo test -p subscript-codegen --test golden` passes in debug and
   in release. Its wall time, with the binary already built, is
   recorded before and after the change in
   `specs/tracking/test-cost-review-2026-11-14.md`. The sweep line
   `golden sweep: compared N entries, skipped 0 entries` shows the
   same `N` before and after.
4. The full gate passes. No golden moves.

### 278.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. It found that each
removed test asserted only the facts of rule 1, and reached the same
run path as the sweep. The direct run calls of 27 removed tests equal
the sweep calls for an entry with no hook and no file provider. Four
MINOR findings:

1. Realistic, recorded: the control of rule 4 does not reach the
   stdout extraction of `dev_sweep_run`. A test outside the crate
   cannot build an `AbnormalTermination` (`#[non_exhaustive]`). The
   tracking note states the limit.
2. Realistic, fixed: the tracking note had sentences in the command
   form. They now state facts.
3. Cosmetic, fixed: a double blank line in the tracking note.
4. Process, fixed: the full gate verdict is in the tracking note.

### 278.4 Open

None.
