# The cost of the JIT `for…of` copy — `compiler.md` §221.4 item 5

Status: MEASURED. A measurement round (Step 0). It lands no production
change.

## Question

§221.1 rule 12 makes the binding of a value element in `for…of` a copy
on each tier. §221.4 item 5 records the cost on the JIT: 100,000,000
visits of 260 bytes take 1.12 s against 0.70 s on a debug CLI. The note
of §221 (`specs/tracking/array-element-receiver.md`) does not record
how that figure was taken. This round measures the cost on the release
CLI at six element sizes.

## Rule that the prototype contradicts

§221.1 rule 12, for a loop body that only reads the binding. The
prototype JIT binds a value element by its address into the array
storage, with no copy. The prototype skips the copy in each loop; the
measured programs have no write to an array in the body, so they
measure the read-only case of the proposal. It does not measure the
cost of a check of the body. The prototype is reverted.

## Method

- Commit `834116f9`, the release CLI (`cargo build --release --bin
  subscript`), on an aarch64 macOS host. One build at a time, and no
  other measurement in parallel.
- The program: a `@ValueType` class `V` with `N` fields of `i32`, so the
  element is `4N` bytes: 2, 4, 8, 16, 32, and 65 fields give 8, 16, 32,
  64, 128, and 260 bytes. `main` pushes 1,000 elements, then runs
  100,000 rounds of `for (const v of all) { total += v.f0; }`:
  100,000,000 visits. It prints `100000000`.
- JIT: the wall time of `subscript run`, by `/usr/bin/time -p`. The
  compile time of the program is under 0.01 s (a run with 1,000 visits
  reads `real 0.00`).
- Ship: `subscript build --source` with the release runtime, then the
  wall time of the executable. The ship tier is the reference: it is not
  changed.
- 3 runs per point; the table states the median. The first run of a new
  executable waits about 0.3 s on macOS, so the first ship run of each
  size reads 0.70 s to 0.76 s. The first series of the prototype at 8
  bytes read 0.57, 0.66, and 0.65 s, after the relink of the CLI; a
  repeat of 8 and 16 bytes read 0.53 s three times each, and the table
  uses the repeat.

## Results

Seconds, median of 3 runs, 100,000,000 visits:

| Element | JIT, copy | JIT, address | copy / address | ship |
|---|---|---|---|---|
| 8 bytes | 0.53 | 0.53 | 1.00 | 0.47 |
| 16 bytes | 0.53 | 0.53 | 1.00 | 0.46 |
| 32 bytes | 0.51 | 0.53 | 0.96 | 0.47 |
| 64 bytes | 0.57 | 0.53 | 1.08 | 0.47 |
| 128 bytes | 0.65 | 0.53 | 1.23 | 0.45 |
| 260 bytes | 0.87 | 0.54 | 1.61 | 0.47 |

All runs, seconds:

| Element | JIT, copy | JIT, address | ship |
|---|---|---|---|
| 8 bytes | 0.53 0.54 0.53 | 0.53 0.53 0.53 | 0.75 0.47 0.47 |
| 16 bytes | 0.53 0.53 0.53 | 0.53 0.53 0.53 | 0.72 0.46 0.41 |
| 32 bytes | 0.51 0.51 0.51 | 0.53 0.55 0.53 | 0.70 0.47 0.42 |
| 64 bytes | 0.57 0.57 0.57 | 0.53 0.53 0.53 | 0.74 0.45 0.47 |
| 128 bytes | 0.65 0.65 0.65 | 0.53 0.53 0.53 | 0.72 0.45 0.44 |
| 260 bytes | 0.87 0.87 0.87 | 0.54 0.54 0.54 | 0.76 0.47 0.47 |

## Reading

At 64 bytes or less, the copy costs 0.04 s or less per 100,000,000
visits, 0.4 ns per visit or less, and the loop itself costs about
0.53 s. That cost is small. At 128 bytes the copy adds 23%, and at 260
bytes 61% (3.3 ns per visit). The ship tier does not depend on the
size. The release figure at 260 bytes, 0.87 s against 0.54 s, has the
same ratio as the debug figure of §221.4 item 5 (1.12 s against
0.70 s).

## Revert

The prototype was one condition in
`codegen/src/lower/func/iterator.rs` (`iterator_value`). It is reverted,
and the release CLI is built again from the reverted tree: 260 bytes
read 0.88 s.
