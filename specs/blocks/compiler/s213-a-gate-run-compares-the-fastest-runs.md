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
1.87x and the ratio of the fastest runs was 1.32x. §3 states that noise
does not trip a limit. That statement is false at this load.

§3 names two answers: more headroom and a serialized run. `cargo test`
already runs the perf gate alone in its test binary, and the load comes
from outside the test run. Headroom over 5.03x removes the check. So
neither answer applies.

### 213.1 Rules

1. A `--gate` run compares the minimum of the timed runs of each
   subject. A ratio is the minimum of the subject over the minimum of
   the C baseline. `dev-iteration` and `hot-reload` compare their
   minimum against 20 ms.
2. Each limit of §3 stays. A deterministic slowdown moves each timed
   run, so it moves the minimum: a 32% slowdown of every run of `a22`
   ship-tier misses 1.50x.
3. A slowdown of only some runs of a subject does not move the
   minimum. A deterministic workload has no such slowdown.
4. A reporting run keeps the median, the spread rule, and the void of
   §9. A number that this project publishes comes from a reporting
   run.
5. The threshold lines of a `--gate` report state that they compare the
   minimum. The table of medians stays in both runs.
6. A miss stays a stop: a perf miss is not re-run by repetition.
7. A gate can still miss on a loaded machine. At two busy processes
   per CPU, the minimum of `a22` dev-JIT is 47x to 55x against 25x, because
   each of its runs lasts 77 ms and the scheduler preempts each one.

### 213.2 Acceptance

1. Red first, at `1ffbb18d`: the samples of the §212 miss give 1.87x
   and miss under the median rule. The tracking note records it.
2. A unit test gives the §212 samples to a gate run and a reporting
   run. The gate run meets 1.50x, and the reporting run misses it.
3. A control multiplies each ship-tier sample by 1.32 from a quiet run.
   The gate run misses 1.50x.
4. A rerun of the load measurement gives each load the verdict that the
   minimum predicts. The tracking note records it.
5. §3 and §9 cite this section.
6. No golden moves.
