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


## Where the debug step's wall time goes (measured 2026-09-05)

The gate on the §86 Task B tree: debug step 1,443 s wall, and the
sum of every `test result: ... finished in` timer 489 s; release
629 s wall against 593 s. The workspace has no doctest; the doctest
phase is 68 s of the debug gap. Every debug test binary, run alone
after `cargo test --no-run`, showed a fixed 30–65 s before its first
test (58 binaries, 1,412 s wall against 499 s of timers); a
17 MiB binary with two tests took 64.9 s the first time and 0.00 s
the second. Reproduced by relinking one binary: first run 29.0 s
inside the tool sandbox and 35.9 s outside it, second run 0.00 s.

**Cause: the host's first-launch check of a newly linked
executable** (macOS 26.6, arm64), paid once per binary per relink,
not by the tests. A change in `compiler/` relinks every dependent
test binary, so a debug gate after such a change pays about 58 ×
30 s. The release step shows a small gap; its binaries are smaller.

Not a defect of the tests or the script. Two candidates, the
owner's decision: fewer test binaries (one `tests/main.rs` per
crate with `mod` per file; the count is what the host charges), or
a host setting that exempts `target/` from the check. Recorded, not
changed.

## Round 4 (2026-09-06) — the cases read the checkout

The first landing that moves a golden (§88, the LIR text golden)
made nine `cli/tests/gate.rs` cases fail: they ran the real `git
status` and reported `goldens-moved 1` against a hand-written `0`.
The day before, a commit during a gate had failed one case the same
way (the stub run's `HEAD` and the test's `HEAD` differed). Both are
one defect: a test that reads the checkout's state is a test the
checkout can fail. Contract `d47eabc`: every case runs with a `GIT`
stub; the expected verdict is a literal.

Landed at `a4f6001`; the §88 gate above ran the twelve cases green with the stub.

## The first-launch cost, root cause (2026-09-06)

The 30–65 s first launch was not the host's check of a new binary
and not the binary's location: a fresh target directory inside the
project launched the same 10 MiB binary in 0.46 s, and a copy of the
slow binary launched from `/tmp` in 0.54 s. `target/debug/deps` held
710,711 entries, of which 707,606 were `.rcgu.o` object files
(26 GB, the oldest from 2026-07-22). With those files moved aside
(`target/_rcgu-aside`, reversible), a relinked binary launched in
0.64 s and a relinked 35 MiB `cemit` test in 0.68 s; `sample` showed
the slow launch entirely inside `_dyld_start`. The mechanism inside
dyld is not identified; the directory size is the measured cause.

Why the files accumulate: on macOS the dev and test profiles default
to `split-debuginfo = "unpacked"`, so every link leaves its codegen
unit objects in `deps/` for the binary's OSO references, and cargo
never removes them; a fresh target directory held 1,048 after two
builds. Measured alternative: `split-debuginfo = "off"` leaves none,
same binary size (10 MiB), OSO references into the rlibs remain.

Gate on the cleaned directory: debug step 543 s (against 1,443–1,728
s before), release 646 s. The earlier record above that attributed
the cost to the host's first-launch check is superseded by this
entry.

## Rule 7a landed at `40a67a6` (2026-09-06)

`[profile.dev] split-debuginfo = "off"` in the workspace
`Cargo.toml`. The coding agent measured: `.rcgu.o` in
`target/debug/deps` 2,504 before and after a full workspace build
and a relink (no growth); a relinked test binary's first run 0.45 s,
second 0.01 s; 364 OSO references, 320 into rlibs, 44 to files that
no longer exist (the pre-change loose objects; harmless to
execution). The owner also approved the deletion of
`target/_rcgu-aside` (707,606 files, 154 s; `target/` 33 GB → 21 GB).

Gate:

```text
gate full ff3a53caf1fea2241f90a559e4a7986e8847121a dirty:1 debug 1286/0/2 release 1284/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Step wall seconds: debug 518, release 602, clippy 16, hygiene 23 —
about 19 minutes for `full`, against about 38 before. The debug
step's tests alone were 489 s, so the untimed part is now about
30 s. The question "run the debug profile less often" is closed by
this measurement: both profiles stay in every landing gate.
