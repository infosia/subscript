<!-- §281 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 281. The generic matrix checks each control and each record once

*(Added 2026-11-14.)* Origin: a test cost review of 2026-11-14 at
`8a36c8c1` (item P1, "reduce generic-matrix setup and repeated record
validation"). Iteration speed is a goal, and a gate's cost is part of
its design (core principle 15).

Problem, measured on 2026-11-14 in debug, with timing added to a copy
of `compiler/tests/generic_tsc_matrix.rs` and then removed. The main
matrix has 40,250 cells and took 35.1 s and 35.4 s in two runs:

| Part | Seconds |
|---|---:|
| Cell generation | 15.5 |
| Writing the temporary files | 3.1–3.3 |
| `tsc`, in parallel with the Rust workers | 12.8–12.9 |
| Rust workers | 13.0 |
| `check_record`, active wall time inside the workers | 8.1–8.2 |
| Deleting the temporary project | 3.0–3.4 |

1. Cell generation runs `check_program` on per-instance control
   programs, one after another on one thread
   (`generic_tsc_matrix/product.rs`, `api.rs`, `destinations.rs`). A
   map removes repeats inside one generator only.
2. Each worker checks the control program of its cell again
   (`check_cell`, file name `concrete.ts`), so a control that
   generation checked is checked twice.
3. The workers call `check_record` 88,666 times for 27 distinct
   record and token pairs. Each call reads and searches a spec file.

### 281.1 Rules

1. One run of the matrix resolves each distinct record and token pair
   once. Each cell reads the result of its pairs. The direct tests of
   `check_record` and `check_record_text` stay as they are.
2. One run of the matrix checks each distinct control program once.
   Generation and `check_cell` read the same result. The cache key is
   the source text and the file name. If the result depends on the
   file name, the two readers use one file name; the coding agent
   states which name, and that no observation reads it.
3. Generation checks its control programs on a bounded pool, at most
   `std::thread::available_parallelism()` workers. A generator first
   collects its candidate controls, the pool checks the distinct ones,
   and then the generator selects in its present order from the
   results. The generator's choice of each cell does not change.
4. The cell set does not change: the same names, sources, divergence
   records, and control sources, in the same order.
5. Each comparison of a cell, and each failure kind that the matrix
   reports, stays as it is.

### 281.2 Acceptance

1. Before the change, a temporary dump of the cell list (name, source,
   divergence record, control source) of each matrix is written and
   hashed. After the change, the hash of each is the same. The tracking
   note records both hashes and the cell counts. The dump code does
   not land.
2. A control for rule 1: the test that a record which does not state
   the restriction fails the cell still fails it through the cached
   path.
3. `cargo test -p subscript-compiler --test generic_tsc_matrix` passes
   in debug and in release. Its wall time, with the binary already
   built, is recorded before and after the change, with the parts of
   the table above, in `specs/tracking/test-cost-review-2026-11-14.md`.
4. The full gate passes. No golden moves.

### 281.3 Phase Review

Pending.

### 281.4 Open

None.
