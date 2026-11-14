# Test cost review, 2026-11-14

Source: a test cost review at `8a36c8c1`. The baseline is the full gate
record of 2026-11-13 at `8a36c8c1`, on `aarch64-apple-darwin`:

```text
gate full 8a36c8c1cd5a35841b7b89a0464b42198d2e74df clean debug 3418/0/5 release 3415/0/5 skips 2/0 clippy 3/10/13 goldens-moved 0 exit 0
```

| Step | Wall seconds |
|---|---:|
| fmt | 2 |
| build | 55 |
| debug tests | 589 |
| release tests | 589 |
| clippy | 21 |
| tsc | 1 |
| hygiene | 3 |

The largest suites in that record:

| Suite | Debug s | Release s |
|---|---:|---:|
| `codegen/tests/lir.rs` | 9.03 | 173.57 |
| `cli/tests/gate.rs` | 46.27 | 46.01 |
| `codegen/tests/golden.rs` | 42.88 | 27.28 |
| `examples/tests/gate.rs` | 59.96 | 4.47 |
| `subscript-codegen` library tests | 45.56 | 9.80 |
| `compiler/tests/generic_tsc_matrix.rs` | 34.72 | 16.75 |
| `codegen/tests/boundary_scratch_breadth.rs` | 36.57 | 2.38 |
| `codegen/tests/long_string_constants.rs` | 33.42 | 3.90 |
| `compiler/tests/robustness.rs` | 29.13 | 3.36 |

## §276 — the gate script tests

The contract pin is `2165c68ae08fd089990a7d92310c9913cf51d2b2`.
The coding agent built the test binary before each debug measurement.

| Check | Command | Result | Wall seconds |
|---|---|---|---:|
| Before the change | `cargo test -p subscript-cli --test gate` | 14 passed; 0 failed | 45.30 |
| Red control at the pin | `cargo test -p subscript-cli --test gate full_fails_on_hygiene_failure -- --exact` | 0 passed; 1 failed; exit 101 | 5.15 |
| After the change | `cargo test -p subscript-cli --test gate` | 16 passed; 0 failed | 16.60 |
| Release | `cargo test -p subscript-cli --test gate --release` | 0 tests; no `gate-skip:` line | 0.51 |
| Format | `cargo fmt --check` | Exit 0 | 2.09 |
| Clippy | `cargo clippy -p subscript-cli --all-targets` | Exit 0; dependency warnings | 11.48 |

`/usr/bin/time -p` measured each wall time.
The red control added only the new case to the pin.
The gate script still used the real hygiene command at that point.
The output was:

```text
running 1 test
test full_fails_on_hygiene_failure ... FAILED

assertion `left == right` failed
  left: Some(0)
 right: Some(1)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 14 filtered out; finished in 4.57s
error: test failed, to rerun pass `-p subscript-cli --test gate`
real 5.15
user 3.55
sys 0.75
```

Each script case now supplies a hygiene stub.
The failure control checks exit 1 and the stub output in the hygiene block.
Case (d) checks the stub path and output after `tsc`, as the last step.
The text check permits the real hygiene path only as the default value.
The release test binary contains no tests.
The full gate result is below. The expected moves are debug +2 (the two new cases) and release −14 (the whole suite, by rule 5).

## §277 — the a22 interpreter run

The contract pin is `2d829b85423d2078884976af331d0fac68d48a2f`.
The agent built the release LIR binary before the baseline run.
`/usr/bin/time -p` measured the wall times.
Both release measurements set `SUBSCRIPT_FULL_INTERPRETER_SWEEP=1`.
The final release measurement ran after all other checks stopped.

| Check | Result line | Wall seconds |
|---|---|---:|
| Release LIR before | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 177.42s` | 178.06 |
| Red interpreter sweep | `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 66 filtered out; finished in 184.36s` | 184.48 |
| Red golden sweep | `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.68s` | 30.22 |
| Release LIR after, concurrent checks | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.29s` | 4.45 |
| Release LIR after, final | `test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.11s` | 1.21 |
| Debug LIR | `test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.88s` | 53.73 |
| Golden sweep | `test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 35.36s` | 35.56 |
| C emission sweep | `test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 25.70s` | 35.33 |
| Corpus suites | `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.65s` | 54.61 |
| Corpus suites | `test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s` | 54.61 |
| Corpus suites | `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.19s` | 54.61 |
| Corpus suites | `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s` | 54.61 |
| Language reference | `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 610 filtered out; finished in 0.12s` | 42.87 |
| Capture tool | Exit 0 | 0.64 |
| Document generator | Exit 0 | 37.30 |
| TypeScript, repository config | Exit 0 | 0.99 |
| Format check | Exit 0 | 11.47 |

The corpus rows share one command and its total wall time.
The debug, corpus, reference, and generator times include build lock waits.
The other final checks ran at the same time.
The red interpreter run failed because a448 had no golden.
The red golden sweep passed. It selects entries with existing goldens.
`compiler/tests/js_corpus.rs` reports a missing golden for each accept
entry. With the a448 golden moved aside, it failed with
`a448-matrix-propagation-witness.ts: missing golden`.

The capture tool produced `256.06827` and a final newline.
The golden sweep confirmed equal dev, ship, and golden bytes.
The interpreter sweep confirmed equal interpreter and golden bytes.
The witness body differs from a22 only in the two required constants.
The a22 golden did not change.
The debug LIR run printed no `gate-skip:` line.
The generator changed only the corpus index.
The index shows the a22 exclusion reason and a448 as `yes`.
The full gate result is below.

## Full gate after §276 and §277

One full gate ran on the §277 tree, above the §276 commit:

```text
gate full c93fbea123d3ed0a08ea25546ea5b0457254ff0a dirty:6 debug 3420/0/5 release 3401/0/5 skips 1/0 clippy 3/10/13 goldens-moved 0 exit 0
```

The counts move as stated: debug +2 and release −14 (§276), and the
debug skip count falls from 2 to 1 (§277). No golden moved.

| Step | Wall seconds, 2026-11-13 | Wall seconds, this gate |
|---|---:|---:|
| fmt | 2 | 2 |
| build | 55 | 3 |
| debug tests | 589 | 388 |
| release tests | 589 | 184 |
| clippy | 21 | 3 |
| tsc | 1 | 0 |
| hygiene | 3 | 4 |
| Sum | 1,260 | 584 |

The build state differs between the two gates. The earlier gate built
the release tests in its release step, and this gate found them built.
So only the suite times above are comparable, not the step sums.

## §278 — the golden sweep

