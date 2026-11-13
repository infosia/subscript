<!-- §269 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 269. A boundary graph walk visits each struct once

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R10.
Selected by the owner on 2026-11-13.

Problem, read at `eebed2d0`. `builds_cycle` (`boundary/src/pass.rs`) finds
whether the scratch build of a struct reaches a struct that is on the
build path. It keeps the active path only. When a call returns, it
removes the struct from the path and keeps no record that the struct
has no cycle. So a graph with no cycle, where several fields reach the
same struct by different paths, is walked once per path. With two links
from each level to the next, the walk of depth `d` costs about `2^d`
calls. `cycle_structs` starts the walk from each struct, and header
binding (`HeaderStructs::new`, `bindgen/src/read_lowering/view.rs`)
runs it. A compact header with a shared aggregate graph can make
`subscript bind` slow. The review names more walks with an active path
only: the lowerability walk of bindgen and the read validation.

### 269.1 Rules

1. Each walk over the struct graph of a header visits each struct at
   most once for each state that changes its result. A struct whose walk
   ended with no cycle is recorded, and a later visit returns the
   recorded result.
2. If the result of a walk depends on more than the struct (for
   example the read root, or the reach mode), the record key holds each
   such input.
3. A diagnostic that names a cycle path keeps the path that it names
   now.
4. The results do not change: each mirror, each pass, and each
   diagnostic stays the same.
5. The implementation lists each walk of `boundary/` and `bindgen/` that
   recurses over struct fields, and states for each one how rule 1
   holds.

### 269.2 Acceptance

1. Red first, at the pin: a unit test builds a struct graph with no
   cycle, of depth 24, where each level has two fields that reach the
   next level. It counts the calls of `builds_cycle` (or measures the
   time), and the count grows as `2^d`. The tracking note records the
   count or the time at depth 16, 20, and 24.
2. After the change, the count is at most the number of structs times a
   small constant, and the test asserts that bound at depth 24.
3. A control: a graph with a cycle still reports `Cycle`, with the same
   path in the diagnostic.
4. The same kind of test for each other walk of rule 5 that can repeat
   work.
5. No corpus entry, golden, or generated mirror changes.

### 269.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The review read each memo: a struct enters the record
only after its walk ends, so a struct on the active path is never in
it, and a cycle path names the first struct found, as before. Seven
walks keep a visited set in place of a path; each stops at its first
result, so the first result does not change. The search for other walks
over struct fields found none.

The first round measured 131,071, 2,097,151, and 33,554,431 calls at
depth 16, 20, and 24 at the pin, and 25 calls at depth 24 after the
change.

1. MINOR, realistic, fixed: names and a doc comment said path where the
   walk keeps a visited set.
2. MINOR, contrived, fixed: no test read the memo that `cycle_structs`
   shares across roots, with a cycle in the graph.
3. MINOR, contrived, recorded: §269.4 item 1.

### 269.4 Open

1. Each record is a list with a linear search, so a walk over `n`
   structs costs up to `n²` comparisons. A header has few structs.
