<!-- §102 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 102. A test waits on a fact, not on a clock

*(Owner decision 2026-09-09.)* Origin: `gate quick` gave three
verdicts on one clean tree, and the owner asked whether a test needs a
deadline at all.

### 102.1 What the investigation found

`cli/tests/watch.rs` `wait_for_count` bounds its wait with
`Instant::now() + Duration::from_secs(20)`.

**The constant has no recorded basis.** It entered with `27e64bd`, the
commit that added `run --watch`, which records the 150 ms polling
interval and states no deadline. `specs/blocks/cli.md` §12 states no
timeout, latency or budget. The number appears in that one line and
nowhere else. A failure at 20 seconds therefore cannot be read: it
says "stuck, or merely busy" and the reader cannot tell which.
Measured, the test passes alone in about 1.2 seconds and consumed the
whole deadline twice under gate load.

**A bound is nonetheless necessary, for a reason the constant does not
serve.** `tools/gate.sh` has no timeout, so nothing above the test
stops a hang. And the capture's reader thread breaks on end of input
without notifying its condition variable, so a watch process that dies
leaves the waiter blocked with nothing left to wake it. The deadline
is the only thing that turns a dead child into a diagnosable failure.

### 102.2 Rule

1. A test that waits for output from a spawned process waits on a
   fact the test already has. End of input is such a fact: the reader
   records it and notifies, and the waiter returns an error naming
   what it wanted and what it received. That error is derived, so it
   needs no number.
2. A wall-clock deadline is not a substitute for rule 1 and does not
   express a latency the product owes. A test states no performance
   bound unless a contract gives it one to state.
3. **The test keeps no wall clock.** *(Corrected 2026-09-09; the
   round found the citation false and stopped.)* This rule first said
   a clock could stay if its value were derived from `cli.md` §12's
   150 ms poll. §12 states no such number: line 345 reads "the
   interval is implementation-chosen and not contracted". The 150 ms
   came from `27e64bd`'s commit message, and writing it here as a
   contract's number was the same defect this section exists to
   remove — a number with no stated source.

   Nothing is available to derive a clock from, and §12 made that so
   deliberately. Rule 1's end-of-input fact covers a child that dies.
   What remains is a child that stays alive and silent, which is a
   hang.

3a. **A hang belongs to the harness, and this repository has none.**
   `tools/gate.sh` sets no timeout, so a hanging test hangs the gate.
   That is a real gap and it is recorded here rather than hidden
   inside one test's constant. It is not this section's work: a
   per-suite bound is one decision for every test, and a test that
   invents its own is the thing rule 2 forbids.

3b. *(Decided 2026-09-10.)* **The gate bounds each step, and the value
   is chosen for generosity rather than derived.** Rule 2 forbids a
   test asserting a latency because a test-level deadline sits in the
   assertion path: when it fires the reader cannot tell "broken" from
   "slow". A step bound sits outside every assertion, so the only
   reading it admits is "something hung". That is why an arbitrary
   number is right here and wrong there.

   The number needs one property, and it is not a source: it must sit
   far above any healthy run **on every host**. Measured on aarch64
   macOS at `a31e593`, the longest step is 561 wall seconds and the
   longest single test binary is 144. Measured on windows-msvc on
   2026-09-06 (`specs/tracking/windows-portability.md`), the release
   step is 1,257 and debug is 875. `tools/gate.sh` bounds each step at
   3600 seconds, just under three times the slowest step on any host,
   overridable with `GATE_STEP_TIMEOUT`.

   *(Corrected 2026-09-10 before it landed. The bound was first 1800,
   chosen against this host's 561 alone, which is 1.4 times the
   Windows release step. The owner asked whether the mechanism works
   on Windows, and the same check found the value.)*

3c. **On a host with no signal delivery path, the bound reports rather
   than pretends.** `windows-portability.md` records it: a native
   parent starts the script, so MSYS `kill` cannot map that process
   id. `kill -TERM` gives `No such process`; `kill -W -f -TERM` ends
   the process through the Win32 interface and runs no trap. §85.3
   item 1 case (i) is already scoped to a POSIX host for this reason.

   So the marker records only that the bound elapsed. The step's own
   exit says whether anything stopped. A step that outran the bound
   and still succeeded emits `gate-timeout-unenforced` and does not
   fail: nothing stopped it, and a record that claimed otherwise would
   be false. A step that outran the bound and failed emits
   `gate-timeout` and fails.

   A hang on a host without delivery therefore stays a hang. That is
   the honest state, and §85's signal rules already carry the same
   limit.

   A step the bound stops is a gate failure with its own line in the
   record, naming the step and the bound. The record says the step
   hung; it says nothing about how fast the step ought to be.

   A per-test bound stays forbidden. Two tests in this suite already
   run over 60 seconds and one binary takes 144, so any number small
   enough to catch a hang quickly would kill work that is merely
   slow.
4. This rule is about waiting, not about timing. A benchmark that
   measures duration is unaffected.

### 102.3 Corpus and gate (pre-registered exit criteria)

Red first: `wait_for_count` blocks forever when its child dies before
producing the needle. Demonstrate it with a spawned process that exits
early, and record that the test hangs rather than failing.

1. The reader notifies on end of input and records that it happened.
   `wait_for_count` returns an error that names the needle, the count
   it wanted, and everything captured.
2. A test that reaches rule 1's error is a firing control: a child
   that exits before producing the needle fails with that message
   rather than hanging or timing out.
3. No wall clock remains in the test. If some shape still hangs after
   rule 1, report the shape rather than adding one back.
3a. `tools/gate.sh` bounds each step per rule 3b. A `cli/tests/gate.rs`
   case proves the bound fires: a stubbed step that sleeps past a
   small `GATE_STEP_TIMEOUT` fails the gate, and the record names the
   step and the bound. Without that case the bound is unfalsifiable.
4. Gates: `tools/gate.sh full` green in both profiles. Run
   `tools/gate.sh quick` three times and report all three verdict
   lines; the flake this section came from must not reappear.
