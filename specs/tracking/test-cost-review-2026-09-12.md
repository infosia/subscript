# Test cost review (2026-09-12)

Status: **measured at `12b310c`; round 1 open.** Origin: the owner
asked which tests are redundant and which take too long.

## Where the gate's time goes

`tools/gate.sh full` at `12b310c` (`target/gate/20260912T033035Z-full.md`):
debug 488 s, release 552 s, everything else 23 s. Five test binaries
hold most of it. Each binary ran alone with `--test-threads=1`, so
the per-test column is the test's own time.

| Binary | debug | release | The test that holds it |
|---|---:|---:|---|
| `codegen/tests/golden.rs` | 158 s | 134 s | the golden sweep: 146 s debug, 130 s release |
| `codegen/tests/lir.rs` | 9 s | 185 s | the interpreter sweep: 185 s, release only |
| `codegen/tests/cemit.rs` | 76 s | 70 s | the trap-corpus sweep: 37 s, 35 s |
| `codegen/tests/long_string_constants.rs` | 73 s | 15 s | the six-length case: 73 s debug |
| `cli/tests/gate.rs` | 37 s | 38 s | eight `full`-shape cases, 3 s each |
| `codegen/tests/boundary_scratch_breadth.rs` | 35 s | 2 s | one test |
| `examples/tests/gate.rs` | 18 s | 7 s | the example sweep 7 s; two host builds |

The 34 named tests in `golden.rs` sum to 26 s and run in parallel
with the sweep, so they add no wall time today.

## The unit cost of one ship-tier run

Measured by hand on `a01-hello` (22 KB of C) and `a204` (370 KB of C),
with the flags `ship.rs` uses:

| Step | a01 | a204 |
|---|---:|---:|
| `cc -O2 -c program.c` | 0.03 s | 0.06 s |
| `cc -O2 -c entry.c` | 0.03 s | 0.03 s |
| link with `target/debug/libsubscript_runtime.a` | 0.06 s | 0.06 s |
| first run of the linked file | 0.40 s | 0.44 s |
| second run of the same file | 0.003 s | 0.003 s |

The first run of a new executable file waits about 0.35 s at 0 % CPU.
A copy of the file pays it again. `int main(void){return 0;}` pays
0.32 s. The wait is the same outside this session's sandbox (0.36 s).
It is a property of this macOS host, per new file, not of the emitted
program.

Consequence: a ship-tier run costs about 0.5 s, and 0.35 s of it is
idle. The gate runs about 450 ship-tier runs per profile (static
count), so about 300 s of a `full` gate is this wait. The golden sweep
confirms the figure: 233 entries in 146 s is 0.63 s per entry, and the
dev-only reload sweep over the same entries takes 0.04 s per entry.

**The wait is serialized by the host.** Eight fresh trivial
executables launched at once take 3.07 s; launched one after another,
2.96 s. Eight fresh `.dylib` files loaded by one warm process with
`dlopen` take 2.94 s the first time and 0.006 s the second. So the
cost is per new Mach-O file, executable or library, one at a time,
whatever the caller's parallelism. A worker pool cannot go below
0.37 s times the number of fresh files; the golden sweep's floor is
about 86 s for 233 entries. Below that floor, only fewer fresh files
per gate help: the duplicate ship runs listed under "Redundant tests"
(about 50 per profile), or one linked file per sweep instead of one
per entry.

## The sweeps are sequential loops

`golden.rs:844`, `lir.rs:1386`, `cemit.rs:650`, and `reload.rs:607`
each iterate the corpus in one `for` inside one `#[test]`. The wait
above is idle, so an eight-worker loop on this host (8 cores) removes
most of it. cargo already runs the named tests in the same process
while the sweep runs, so concurrent JIT and ship runs in one process
are exercised today. One entry is not thread-safe: `HOST_OWNED_STATE_ID`
uses the fixture's process-global C state
(`codegen/tests/support/native_fixture.rs:108`).

## The release interpreter sweep is one entry

Round 1 measured every entry of `lir_interpreter_profile_matches_corpus_goldens`
in release (a temporary per-entry probe, reverted):
`a22-matrix-propagation` takes 185.2 s of the sweep's 185.3 s; the
other 176 entries sum to about 1.2 s. `a22` carries `cost: benchmark`
(§88 rule 2); the debug sweep omits it and the release sweep runs it
(§88 rule 3). A pool cannot shorten one item. Whether the reference
interpreter must run a benchmark entry at all is the owner's decision:
an `interpreter: no — cost` header on `a22` removes 185 s from every
`full` gate and keeps the dev and ship tiers on the entry.

## `long_string_constants.rs`, per case (debug)

| bytes | Literal, plain | Literal, mixed |
|---:|---:|---:|
| 64,999 – 65,536 (5 lengths) | 2.0 s each | 2.1 s each |
| 1,048,576 | 24.0 s | 26.2 s |

The two 1 MiB cases are 50 s of 73 s. §99's exit criteria name all six
lengths, so the lengths stay. The 13 cases are independent.

## `cli/tests/gate.rs`

The script with every tool stubbed runs `quick` in 0.4 s and `full`
in 1.8 s. The difference is `tools/hygiene.sh`, which the script runs
unstubbed (§85.1 rule 7 names no `HYGIENE` variable). Eight cases run
the `full` shape. The `sleep 3` and `sleep 20` stubs are §85.3 (i) and
§102 rule 3b cases and stay.

A claim that the watchdog's `wait` costs one second per step was
measured false: five steps in 0.007 s.

## `boundary_scratch_breadth.rs`

`checked(false)` and `checked(true)` (line 321) check the two sources;
`run_jit_with_native_libraries` (line 389) checks both again from
source, and line 384 drops the HIR it could have used. `jit.rs` has no
entry that takes a HIR. Two of four checks are avoidable.

## `examples/tests/gate.rs`

The two host cases run `examples/host/build.sh` and
`examples/context-per-scene/build.sh`, and each script runs
`cargo build --offline --release -p subscript-cli` inside the debug
test step. Alone, the binary takes 10 s; in the gate's debug step, 18 s.

## Redundant tests

A sweep already runs these inputs on the same tiers:

- `golden.rs`: the 45 entries the named tests run are all golden
  sweep entries. Cost: 45 ship-tier runs per profile.
- `cemit.rs:840` (a74–a76 against hard-coded bytes),
  `interop.rs:271` (t46, the same call pair as `cemit.rs:707`),
  `interop.rs:260` (a90), `surrogate_continuations.rs:6` (LF half).
- `lir.rs:889`, `:1545`, `:2192` check and lower the accept corpus
  three times for three read-outs.
- `compiler/tests/corpus_accept.rs:118`, `corpus_warn.rs:125`,
  `operation_signatures.rs:255` check the accept corpus three times;
  `corpus_reject.rs:331` and `:371` check the reject corpus twice.
  Under 1 s each; a form finding, not a time finding.
- `compiler/tests/synthetic_prefix.rs:93` includes a176 and a177 and
  asserts only `Ok`.
- `compiler/tests/tsc_corpus.rs` starts `tsc` four times;
  `cli/tests/commands.rs:621`, `:672`, `:735` start the binary twelve
  times for one property.

## Tests that cannot fail

- `examples/tests/gate.rs:455` re-checks the filter at line 122 and the
  prefix at line 148 against their own output.
- `compiler/tests/robustness.rs:43`: a "does not panic" sweep with no
  firing control.
- `compiler/tests/corpus_warn.rs:189`: `assert_eq!(…, 11)` fails on a
  new example, not on a defect.
- Literal copies: `benchmarks/tests/perf_gate.rs:6`,
  `codegen/tests/lir.rs:1375`; trivial: `long_string_constants.rs:52`,
  `cli/tests/gate.rs:186`, `compiler/tests/js_corpus.rs:671`.

## Not a duplicate

The release step's ship-tier half links
`target/release/libsubscript_runtime.a` and the debug step links the
debug archive, so the two are different programs. A proposal to run
the ship tier in one profile only was rejected on that evidence.

## Round 1 — the sweeps on a worker pool

`codegen/tests/support/pool.rs` (`map_in_order`, standard library
only): a bounded pool of `available_parallelism()` workers, one shared
index, results returned in item order, every worker joined, the first
worker panic resumed on the test thread. The five sweeps hand each
entry to it; the test thread prints and asserts, so the printed order
and the failure text are the sequential loop's. Entries that drive the
fixture's process-global C state (`a128`, `a137` in `reload.rs`; the
three `host_hooks` ids in `golden.rs`) run on the test thread after
the pool.

Measured in the round's worktree, each binary alone, default threads
(release with `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1`):

| Binary | debug before | after | release before | after |
|---|---:|---:|---:|---:|
| `golden.rs` | 163 s | 105 s | 143 s | 97 s |
| `lir.rs` | 11 s | 9 s | 185 s | 183 s |
| `cemit.rs` | 75 s | 58 s | 67 s | 54 s |
| `reload.rs` | 10 s | 3 s | 1 s | 1 s |
| `long_string_constants.rs` | 75 s | 33 s | 16 s | 9 s |

The gate's debug step: 328 s against 488 s. `golden.rs` stops near
the serialized floor above. `lir.rs` release does not move; see "The
release interpreter sweep is one entry".

The orchestrator extended the file set by one line:
`corpus/interop/interop.c` line 81, `static char subscript_msgbuf[256]`
became `_Thread_local` (C11). Three sites `memset` it and pass a view
to a callback; every writer writes `'x'` and every reader reads its
own length, so no output changes, but concurrent writes were a data
race, and cargo's concurrent tests could already reach it.

Fresh review (one no-context reviewer): CRITICAL 0, MAJOR 2, MINOR 4.

- MAJOR 1, the worktree gate ran before the `interop.c` line: closed
  by the landing gate on `main`, below.
- MAJOR 2, "a worker panic escapes libtest's capture": **refuted by
  measurement.** A `rustc --test` probe with a scoped worker that
  prints and panics shows the prints and the
  `thread '<unnamed>' panicked at` line inside the test's
  `---- stdout ----` block; a spawned thread inherits the capture.
  The handoff's rule 1 and the first text of `pool.rs`'s module doc
  stated the same false fact; the comments were corrected. The design
  stays: returning the lines keeps the printed order.
- MINOR, `fork()` from worker threads: `execute_entry_retained`
  (`codegen/src/jit.rs`) forks the process to run the entry, and a
  child of a multi-threaded parent is limited to async-signal-safe
  calls. The class predates the pool (cargo runs tests on several
  threads); the pool raises the concurrent-fork count to the pool
  width. Recorded as a hang class; no failure observed.
- MINOR, `SUBSCRIPT_CODEGEN_JIT_OUTPUT_FILE` in the parent process
  would interleave two workers' output; no test sets it in-process.
  Noted in `map_in_order`'s doc.
- MINOR, `cemit.rs` 2,767 → 2,805 lines and `lir.rs` 2,575 → 2,579,
  both above §5.y's limit before this round.

Landed at `9faf19a`. The gate on `main`
(`target/gate/20260912T084848Z-full.md`; the dirty file is this
note): debug step 358 s against 488 s, release step 476 s against
552 s.

```text
gate full 9faf19aaec09c5f7370d56ba67e489fa30ce3e16 dirty:1 debug 1457/0/2 release 1455/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

## Rounds

1. Parallelize the four sweeps and the thirteen long-string cases.
   Files: `codegen/tests/golden.rs`, `lir.rs`, `cemit.rs`,
   `reload.rs`, `long_string_constants.rs`. No contract change.
   **Landed; see above.**
2. Share the HIR in `boundary_scratch_breadth.rs`: one JIT entry that
   takes a checked HIR.
3. `HYGIENE` in `tools/gate.sh` (§85.1 rule 7), and the example host
   cases without the nested cargo build. Contract first.
4. The redundant tests and the tests that cannot fail, above.

Owner decision 2026-09-12: no macOS settings change is tried against
the 0.35 s wait. The wait is a host property that the tests must
absorb; the work stays on the test side.
