<!-- §337 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 337. A test states its cost and does no work twice

*(Added 2026-11-19.)* Origin: rank 19 of `specs/tracking/backlog.md`
(§150.3 items 1, 2, §168.3 item 2, §169.3 item 7, §174.3 items 1, 2,
§302.4 item 1). On 2026-11-19 the owner told Claude to start ranks 18
and 19.

Each item is a test that runs work that another test already runs, a
test that builds C once for each value of one input, or a test that
states no measured cost (core principle 15). One item is a test that
reads a file that an earlier run can leave (core principle 9). No
program changes its output.

Measured at `61293f78` by reading the tests:

| Item | Test | Problem |
|---|---|---|
| §150.3 item 1 | `codegen/src/postfix_update_tests.rs` `postfix_corpus_matches_all_three_forms` | runs `a326` in three tiers; the corpus tests run it again |
| §150.3 item 2 | the same file | the ship test builds C and states no cost |
| §168.3 item 2 | `codegen/src/interpreter/budget_tests.rs` `checkpoint_reports_match_jit_ship_and_interpreter` | builds C once for each of three budgets, six builds; only the budget changes |
| §169.3 item 7 | `codegen/src/interpreter/inspection_tests.rs` | optimized C builds, no stated cost |
| §174.3 item 1 | `compiler/tests/operation_signatures.rs` | holds a copy of `compiler/tests/support/lifetime_sites.rs`; the support file has no `ArrayClear` arm and no user |
| §174.3 item 2 | `codegen/tests/zero_length_store.rs` | runs `a344` in three tiers again; two tests build C and state no cost |
| §302.4 item 1 | `codegen/src/reload/scratch_exit_tests.rs` `ship_scratch_allocation_exit` | a run that writes no count reads the result of no run only because a helper deletes the file; the test itself does not show it |

### 337.1 Rules

1. **A corpus entry runs once per tier.** A test that runs a corpus
   entry in a tier where the corpus gate already runs it, with the same
   golden, is removed (§150.3 item 1, §174.3 item 2). Before the
   removal, the coding agent names the gate test that runs the entry in
   each tier. If a tier has no such test, the test keeps that tier only.
2. **An input that changes at run time does not change the build.** A
   differential test that builds C for each value of one input builds
   each program once and gives the input at run time, through `argv`
   or through a host hook (§168.3 item 2). The run of each value stays.
3. **A test with a C build states its cost.** Each test that this
   section changes, and each test of §150, §169, and §174 that builds C,
   has a comment with its measured cost and the count of its C builds.
   An optimized C build states the reason that the test needs it; if no
   reason holds, the test uses the build that the corpus gate uses
   (§169.3 item 7).
4. **One copy of a test support module.** `lifetime_sites` has one
   source. The copy that the test uses stays, with its `ArrayClear`
   arm; the unused file is removed, or the test uses the file and the
   copy is removed (§174.3 item 1).
5. **A run that writes no count fails.** Before each ship run, the
   test of §302 rule 1 writes into the count file a value that no run
   writes. So if the post-run hook does not write, the test reads that
   value and fails, with no extra C build (§302.4 item 1).

### 337.2 Acceptance

1. For rule 1, the report names the gate test that runs `a326` and
   `a344` in each tier.
2. For rule 2, the test builds two C programs, not six, and checks the
   same reports for each budget as at the pin.
3. For rule 5, a run of the test with the hook body empty fails. The
   coding agent measures it once and reverts it.
4. The report gives the measured time of each changed test at the pin
   and after (debug, best of three).
5. No corpus entry and no golden changes. The full gate passes.

### 337.3 Sections this one amends

- §150.3 items 1, 2, §168.3 item 2, §169.3 item 7, §174.3 items 1, 2,
  and §302.4 item 1 close.

### 337.4 Phase Review

Form. The gate tests that run each accept entry against its golden are
`codegen/tests/golden.rs` `jit_ship_c_aot_and_golden_agree_byte_for_byte`
(development tier and ship tier) and `codegen/tests/lir.rs`
`lir_interpreter_profile_matches_corpus_goldens` (interpreter). They run
`a326` and `a344`, so the two tests that ran these entries again are
removed. The budget test passes the budget in `argv`. The ship scratch
test writes `unwritten` into the count file before each run. The unused
`compiler/tests/support/lifetime_sites.rs` is removed.

Measured, debug, best of three, the pin `5c2403f4` and after:

| Test | Pin | After | C builds |
|---|---|---|---|
| `checkpoint_reports_match_jit_ship_and_interpreter` | 1.93 s | 0.82 s | 6 to 2 |
| `postfix_corpus_matches_all_three_forms` | 0.28 s | removed | 1 to 0 |
| `zero_store_corpus_agrees_in_three_tiers` | 0.30 s | removed | 1 to 0 |
| `async_task_snapshots_match_three_tiers` | 2.88 s | 2.96 s | 9 (one per program) |
| `ship_scratch_allocation_exit` | 0.76 s | 0.78 s | 2 |

With an empty post-run hook, the ship scratch test fails: it reads
`unwritten`, not `0`.

One fresh review found no CRITICAL or MAJOR finding.

1. MINOR, realistic, fixed: the test of §302 rule 1 checked only a
   helper that no other test used after rule 5, so it could not fail.
   It is removed; rule 5 shows the rule in the ship test.
2. MINOR, contrived, recorded: §337.5 item 1.

### 337.5 Open

1. If the budget test passes a budget that parses to 0, the ship host
   loop makes no progress, and the test does not end. The test passes
   1, 3, and `u64::MAX` only.
