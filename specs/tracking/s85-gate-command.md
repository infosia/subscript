# §85 — one gate command, two shapes

Status: **landed** at `72fe77e` (contract `67d00d6`, `0746a79`, `81dd964`, `aa4b803`). Contract: `specs/blocks/compiler.md`
§85. Origin: `specs/tracking/development-cost-review-2026-09-05.md`
finding 3.

## What changed

- `tools/gate.sh quick|full`: the gate, its record under
  `target/gate/`, and the verdict line.
- `codegen/tests/lir.rs`, `benchmarks/tests/perf_gate.rs`: the two
  profile skips print `gate-skip:` lines; one test each pins the text.
- `cli/tests/gate.rs`: six stub cases with hand-written verdicts.

## Review round 1

- Contract, forced (`81dd964`): the debug command declares the
  `perf_gate` skip, so one skip total cannot be 0 in `full`. Rule 6
  reports `skips <d>/<r>`; item 3 expects `1/0`. Found by the coding
  agent's report before the round closed.

## Review round 2 (fresh reviewer, read-only)

CRITICAL 0, MAJOR 3, MINOR 9. Contract amended at `aa4b803`:

- MAJOR: every stub case left a record in `target/gate/` beside the
  real ones (22 stub records at review time). Rule: a case deletes
  its record.
- MAJOR: an interrupted `full` run left a record with the last three
  command blocks twice and no verdict
  (`target/gate/20260905T070344Z-full.md`). Rule 5: a signalled run
  deletes its record; case (i).
- MAJOR: no case observed `full` with `exit 0`, and nothing could
  make `goldens-moved` non-zero. Rule 7 adds `GIT`; cases (g), (h).
- MINOR, fixed: the reservation loop on an unwritable directory;
  optional verdict fields printed as zeros when the step did not
  run (rule 6); stub stdout inside a panic message; a poisoned test
  mutex; per-crate baseline controls.
- MINOR, recorded: the expected revision and dirty count come from
  a second `git` call (separate derivation, a flake source under a
  concurrent edit); a step's output is shown only when it ends; the
  Windows host result is pending (§85.3 item 4); each `full` stub
  case runs `tools/hygiene.sh` for real, about 22 s each.

## Gates

`tools/gate.sh full` on the round-3 tree (record
`target/gate/20260905T081302Z-full.md`):

```text
gate full aa4b803c8922e4214756c25cffb2a4571fd940e9 dirty:4 debug 1269/0/1 release 1267/0/1 skips 1/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Step wall seconds, this host (aarch64-apple-darwin): fmt 1, build 4,
debug 1,384, release 803, clippy 60, tsc 0, hygiene 24. The debug
figure holds `boundary_scratch_breadth` (§86 is its section) and the
twelve gate cases, six of which run `tools/hygiene.sh` for real.

## Round 3 facts

- The duplicated command blocks of review round 2 came from an edit
  of `tools/gate.sh` while a run read it; `sh` reads a script by
  offset. The signal path was still incomplete: TERM left the
  partial record. Both are closed: the script deletes its record and
  scratch directory on HUP, INT, or TERM, and case (i) pins it.
- TERM to the script alone does not end a running `cargo` child;
  `sh` runs the trap after the foreground command returns. A
  terminal interrupt reaches the whole process group. Recorded, not
  changed.
- The twelve cases: (a)–(i) of §85.3 item 1, the per-crate baseline
  controls, and a `full` stop at `build` with no `release` or
  `clippy` field.

## Next

§86 (C emission), §87 (synthetic owner), §88 (corpus inventory), in
that order. Every landing cites the verdict line of this script.

