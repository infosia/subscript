# Boundary struct walk memo

Contract: compiler.md §269 rules 1–5.
Pin: `098da5646bd1769460a71992c18effe7ca440af3`.

## Red evidence

Tests used the pin implementation plus test code and test counters.
Each graph has depth 24, 25 structs, and two embedded fields to the next level.
The terminal struct has no fields. Boundary tests use a counting StructView and ReadView.
Bindgen tests use Parsed structs and a test-only function entry counter.
All ten tests failed the bound of four times the struct count at depth 24.

Each test recorded these counts:

| Depth | Red count |
| --- | ---: |
| 16 | 131,071 |
| 20 | 2,097,151 |
| 24 | 33,554,431 |

The first builds_cycle run took 57.693 ms, 565.843 ms, and 10.223 s at these depths.
The first cycle_member run took 94.424 ms, 1.110 s, and 18.700 s.
The complete boundary Red run used six test threads and took 23.92 s.
Its depth-16 times ranged from 59.378 to 106.738 ms.
Its depth-20 times ranged from 738.458 ms to 1.492 s.
The bindgen Red run used four test threads and took 56.15 s.
Its depth-16 times ranged from 163.296 to 215.958 ms.
Its depth-20 times ranged from 2.489 to 3.497 s.
No depth-24 test exceeded 60 seconds.

## Walk inventory and test cost

The table gives each recursive struct-field walk, its rule 1 action, and its depth-24 test cost.
Green measurements used one test thread.
Boundary counts measure field expansions. Bindgen counts include calls that return from the visited set.

| Walk | Action | Red time, s | Green count | Green time, µs |
| --- | --- | ---: | ---: | ---: |
| pass::builds_cycle | Cache completed boolean results; keep the active cycle path | 13.351 | 25 | 16.917 |
| pass::writes_back_from | Retain the visited set for one query | 14.785 | 25 | 14.666 |
| pass::builds_scratch_from | Retain the visited set for one query | 14.676 | 25 | 16.000 |
| read::written_back_member | Retain the visited set for one query | 22.233 | 25 | 24.708 |
| read::Walk::structure | Retain keys with struct, ReadRoot, and Reach | 21.032 | 25 | 23.500 |
| read::cycle_member | Cache only completed paths with no cycle | 22.360 | 25 | 32.417 |
| emit::validate_lowerable_boundary_aggregate | Retain the visited set for one root | 46.951 | 49 | 121.834 |
| emit::collect_recursive_string_owners | Retain the visited set for one collection | 52.431 | 49 | 152.209 |
| completion::validate_struct | Cache successful structs; keep the active cycle path | 40.602 | 49 | 133.166 |
| read_lowering::HeaderStructs::holds | Retain the visited set for one query | 44.606 | 49 | 873.708 |

Each Green test also measures depths 16 and 20. Boundary counts are 17 and 21; bindgen counts are 33 and 41.
Each test asserts the depth-24 bound. The small fixtures cost less than one millisecond each at depth 24.
The holds test includes HeaderStructs construction. No test needs repeated expensive work.

The iterative pass::reachable walk already retains its seen set.
The iterative largest_copying_set embedding walk already retains a seen set for each root.
Its fixed-point byte decision reads a member table; each iteration has a distinct candidate set.
cycle_structs shares completed cycle decisions when the view supplies one fixed byte set.
Without that set, each struct_pass query owns its byte decision and memo.
The bindgen frontend reads declaration fields without a recursive struct-id graph walk.
The callback-lifetime check reads direct fields only.

## Keys and diagnostics

Each query fixes the view, byte set, registry, and selection inputs for its cache lifetime.
The read key includes ReadRoot and Reach, as compiler.md §269 rule 2 requires.
The input-origin flag stays fixed for the query.
A successful read has no diagnostic; its through path cannot change success.
A failed read returns immediately through the original declaration-order path.
Each cycle diagnostic keeps the active path separate from completed acyclic structs.
Completion validation also keeps its active cycle path separate from successful structs.
No diagnostic result is reused under a different root or function.

## Results

- New boundary tests: PASS, 6 tests, 0.00 s test time; 0.38 s compilation.
- New bindgen tests: PASS, 4 tests, 0.01 s test time; 0.03 s build check.
- `cargo test -p subscript-boundary`: PASS, 1.014 s wall time.
- `cargo test -p subscript-bindgen`: PASS, 4.574 s wall time.
- `cargo fmt --check`: PASS, 2.007 s wall time.
- `cargo clippy -p subscript-boundary -p subscript-bindgen --all-targets`: PASS, 0.510 s wall time.

Clippy reports five existing warnings in the unchanged `bindgen/src/emit/tests.rs`.
It reports no warning in a changed file.
The existing cycle control checks self, mutual, ancestor, and pair cycles and their named members.
Both crate suites pass those controls and the existing diagnostic tests.
The bindgen suite passes byte-identical committed mirror regeneration.
No corpus entry, golden, generated mirror, or contract file changed.
No commit, push, or gate script ran.

## Shared-root test and visited names

Retained sets use `visited` in the seven walks that keep visited structs.
The read documentation describes a visited struct at the same root and reach.
Cycle diagnostics and completion validation keep their current path names.

The shared-root test supplies `copying() = Some(&[])` and `cycles() = None`.
Two roots share an acyclic diamond. One root then reaches a two-struct cycle.
A separate recursive walk uses only its current path, with no memo, as at the pin.
The test compares the two cycle-struct sets, as core principle 9 requires.
It asserts one field expansion per struct: 8 calls for 8 structs.
The measured cycle_structs call took 10.375 µs.

- Shared-root test: PASS, 0.089 s wall time.
- `cargo test -p subscript-boundary`: PASS, 1.278 s wall time.
- `cargo test -p subscript-bindgen`: PASS, 4.622 s wall time.
- `cargo fmt --check`: PASS, 2.009 s wall time.
- `cargo clippy -p subscript-boundary -p subscript-bindgen --all-targets`: PASS, 0.603 s wall time.

Clippy reports five existing warnings in the unchanged `bindgen/src/emit/tests.rs`.
It reports no warning in a changed file.
