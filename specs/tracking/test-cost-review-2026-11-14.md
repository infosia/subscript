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

