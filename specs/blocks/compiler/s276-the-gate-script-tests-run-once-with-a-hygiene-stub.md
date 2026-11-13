<!-- §276 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 276. The gate script tests run once, with a hygiene stub

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P0, "stub hygiene" and "run the shell tests once").
Iteration speed is a goal, and a gate's cost is part of its design
(core principle 15).

Problem, measured in the full gate record of 2026-11-13 at `8a36c8c1`
(`debug 3418/0/5 release 3415/0/5`, 1,260 s in total):

1. `cli/tests/gate.rs` took 46.27 s in the debug step and 46.01 s in
   the release step. The suite runs `tools/gate.sh` with stubs for
   `cargo`, `node`, `tsc`, `cc`, and `git`. The Rust profile of the
   driver does not change what the shell script does. So the release
   run observes nothing that the debug run does not observe.
2. `tools/gate.sh` calls `tools/hygiene.sh` directly. Seven cases
   reach the hygiene step, so each profile runs the real hygiene scan
   seven times. The outer gate's own hygiene step took 3 s. The real
   scan also reads the checkout, so a hygiene hit in the tree fails
   the script tests as well as the gate's own hygiene step.

### 276.1 Rules

1. `tools/gate.sh` reads `HYGIENE` from the environment, with the
   default `tools/hygiene.sh`. The hygiene step runs `"$HYGIENE"`.
   The record's command line for that step names the value of
   `HYGIENE`.
2. Each case of `cli/tests/gate.rs` sets `HYGIENE` to a stub that the
   case writes. The stub prints one fixed line and exits 0, unless a
   case gives it another exit status.
3. One case gives the hygiene stub exit 1 in `full`, with each other
   stub at its passing form. The verdict has `exit 1`, and the record
   holds the stub's line in the hygiene block. This case is the firing
   control of rule 2.
4. The case that reads the hygiene block (§85.3 case (d)) reads the
   stub's line and the stub's path in that block. So the case shows
   that the step ran after `tsc`, as the last step of `full`.
5. `cli/tests/gate.rs` compiles only with `debug_assertions`
   (`#![cfg(debug_assertions)]`). In the release step it runs no test
   and prints no `gate-skip:` line. This is a profile policy, not a
   skip: the script's behaviour does not depend on the Rust profile
   of the driver.
6. The real `tools/hygiene.sh` runs once in each full gate, as the
   gate's own last step. No other test runs it.
7. This section replaces the sentence of §85.3 item 1 that says
   `hygiene.sh` runs for real in the script tests. §85.1 rule 7 gains
   `HYGIENE` in its list of overrides.

### 276.2 Acceptance

1. Red first, at `8a36c8c1`: the new case of rule 3 fails, because the
   script ignores `HYGIENE` and the real hygiene passes.
2. `cargo test -p subscript-cli --test gate` passes in debug. The
   wall time of that command, with the binary already built, is
   recorded before and after the change in
   `specs/tracking/test-cost-review-2026-11-14.md`.
3. `cargo test -p subscript-cli --test gate --release` runs 0 tests
   and prints no `gate-skip:` line.
4. A text check in `cli/tests/gate.rs` reads `tools/gate.sh` and
   finds no call of `tools/hygiene.sh` other than the default value of
   `HYGIENE`.
5. The full gate passes. Its record shows the hygiene step with the
   command `tools/hygiene.sh`. The debug and release counts move only
   by the cases that this section adds or removes.

### 276.3 Phase Review

One fresh review. No CRITICAL or MAJOR finding. Three MINOR findings,
each realistic and fixed in the text:

1. §85.1 rule 7 had one line past the wrap width. It is rewrapped.
2. The tracking note had passive sentences and named a file that a
   clone does not have. The sentences are now active.
3. Acceptance item 5 did not state the expected count moves. The
   tracking note states them: debug +2, release −14 (rule 5).

### 276.4 Open

None.
