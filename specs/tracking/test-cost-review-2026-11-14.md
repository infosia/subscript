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

