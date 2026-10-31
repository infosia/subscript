<!-- §213 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 213. A gate run compares the fastest runs

*(Added 2026-10-31.)* Origin: on 2026-10-31 the owner asked for the
open work in priority order. The full gate of §200 and of §212 failed
on the release perf gate with no change in `codegen/` or `runtime/`.
The measurement is in `specs/tracking/perf-gate-fastest-run.md`.

Problem, measured at `1ffbb18d` on an 8-CPU aarch64 host: the gate
compares medians, and a load of two busy processes per CPU moves the
median ratio of `a22` ship-tier to C from 1.34x to between 1.16x and
5.03x. The limit is 1.50x. The ratio of the fastest runs stays between
1.30x and 1.36x at each load. In the §212 miss the median ratio was
1.87x and the ratio of the fastest runs was 1.32x. §3 at `1ffbb18d`
stated that noise did not trip a limit. That statement was false at
this load.

§3 at `1ffbb18d` named two answers: more headroom and a serialized run.
`cargo test` already runs the perf gate alone in its test binary, and
the load comes from outside the test run. Headroom over 5.03x removes
the check. So neither answer applied.

### 213.1 Rules

1. A `--gate` run compares the minimum of the timed runs of each
   subject. A ratio is the minimum of the subject over the minimum of
   the C baseline. `dev-iteration` and `hot-reload` compare their
   minimum against 20 ms.
1a. The figure of a dev-JIT subject is the minimum thread CPU time of
   its timed runs, over the minimum wall time of the C baseline. A
   dev-JIT run lasts 77 ms, longer than a scheduler slice, so its wall
   minimum carries preemption. A host with no thread CPU clock in this
   harness (Windows) keeps the wall minimum. *(Added 2026-10-31 after
   the Phase Review, M1.)*
2. Each limit of §3 stays. A deterministic slowdown moves each timed
   run, so it moves the minimum: a 32% slowdown of every run of `a22`
   ship-tier misses 1.50x.
3. The gate does not see a slowdown that leaves one timed run fast. A
   reporting run sees it. The gate is also less sensitive under load:
   at two busy processes per CPU, the wall minimum of the `collect` C
   baseline rises to 3.4x, so a `collect` dev-JIT ratio reads low. A
   deterministic regression moves the figure at the next gate on a
   quieter machine. This is the trade: a gate gives up these cases to
   stop a false miss, which stops the round. *(Amended 2026-10-31 after
   the Phase Review.)*
4. A reporting run keeps the median, the spread rule, and the void of
   §9. A number that this project publishes comes from a reporting
   run.
5. The threshold header of a `--gate` report states the figure. The
   table of medians stays in both runs.
6. A miss stays a stop: a perf miss is not re-run by repetition.
7. A gate can still miss on a loaded machine. CPU time also grows
   under load: at 1.25 to 2 busy processes per CPU, the figure of `a22`
   dev-JIT is 22.6x to 24.5x against 25x.

### 213.2 Acceptance

1. Red first, at `1ffbb18d`: the samples of the §212 miss give 1.87x
   and miss under the median rule. The tracking note records it.
2. A unit test gives the §212 samples to `write_report` in a gate run
   and a reporting run. The gate run meets 1.50x, and the reporting run
   does not. A deletion of the mode at a compare call fails a test.
3. A control multiplies each ship-tier sample by 1.32 from a quiet run.
   The gate run misses 1.50x.
4. A rerun of the load measurement gives each load the verdict that the
   minimum predicts. The tracking note records it.
5. §3 and §9 cite this section.
6. No golden moves.

### 213.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `1ffbb18d..d6d8e604` gave two
MAJOR and eight MINOR findings.

1. MAJOR, realistic, fixed: the wall minimum of `a22` dev-JIT missed
   25x from 10 busy processes on 8 CPUs, and rule 7 named 16. On the §212
   samples the minimum moved dev-JIT from 20.63x to 22.68x. So the first
   form moved the false miss from one subject to another. Rule 1a.
2. MAJOR, realistic, fixed: no test called `write_report`, so a gate
   run with the median rule passed each test. Acceptance 2.
3. MINOR, realistic, fixed: rule 3 stated with no measurement that a
   deterministic workload has no partial slowdown. It now states a
   blind spot. §116.3 records a bimodal ship `tree` on one binary.
4. MINOR, realistic, recorded: a load that starts or stops between the
   C and the ship subjects gives a ship ratio of 1.42x to 1.43x, or
   1.21x to 1.26x. The tracking note has the runs. Rule 2 holds for a
   constant load.
5. MINOR, fixed: §3 said "above one busy process per CPU". The miss is
   at one per CPU.
6. MINOR, fixed: §3 kept "not a weaker check" with no statement of the
   trade.
7. MINOR, fixed: §3 and §9 kept the 3% variation as the reason that a
   gate needs no quiet machine.
8. MINOR, fixed: the problem statement used the present tense for text
   that this section removed, and §3 "eval median" had no gate
   qualifier.
9. MINOR, fixed: the tracking note did not give the §200 ratios.
10. MINOR, fixed: a test read a literal back, and a test copied 1.50.

### 213.4 Open

1. The `collect` limits are stale. `collect` reads 1.12x ship and 3.63x
   dev against 7.50x and 8.50x, so the gate misses no ship regression
   under about 6.7x, on either figure. §3 derives these limits from
   6.45x and 7.04x. A change of a pre-registered limit needs its own
   section.
2. The Windows path of rule 1a has no build here. No Windows target is
   installed.
