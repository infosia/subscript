# A generic call infers from its contextual result type — `compiler.md` §212

Status: implemented; Phase Review open.

## Measurement at the pin `b1ec9a9b`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(`corpus/node/run-js-corpus.cjs`), and `target/debug/subscript` built
from the pin.

`a397-contextual-result-inference`: `tsc` gives no error, `node` prints
the golden, and subscript gives 13 errors: 12 × S100 "cannot infer type
parameter `T` of … : no candidate; use explicit type arguments" and
1 × S005 for `const widened: Buffer<i64> = wrap(7);`.

| Program | `tsc` |
|---|---|
| `const a = make();`, `a.items.push(1);` | no error: `T` is `unknown` |
| `two<i32>("x")` for `two<T, U>` | TS2558 "Expected 2 type arguments, but got 1" |
| `const x: Particle[] = zero(null);`, `zero<T>(f: T \| null): T[]` | TS2322 "Type 'null[]' is not assignable to type 'Particle[]'" |

The request marks its `tsc` claim as taken from documentation. The
first row of the measured table and `a397` confirm it. The third row
is a case that the request does not name: §212.1 rule 3 exists for it.

The request also names a gap in a downstream entry point. That gap is
not in this repository.

## Implementation

The coding agent measured two forms that the request does not name.
`const x: i32 = -zero();` gives `tsc` TS2571, and `xs[zero()]` gives
`tsc` TS2538. §212.1 rule 1a exists for them. `g(empty(), null)` passes
`tsc`, so rule 3 keeps the context of an argument that the checker
reads before the `null`.

`a397` passes on the three tiers, `tsc`, and `node`. No golden moves.

`gate full d2443368 dirty:14 debug 3035/0/5 release 3031/1/5 skips 2/0
clippy 5/18/11 goldens-moved 0 exit 1`. The one failure is the release
perf gate: `a22 ship-tier 1.87x of C, limit 1.50x`. The eleven C runs
of `a22` spread from 5.02 ms to 12.46 ms (99.0%), and the load average
was 12.57 over 15 minutes. `git diff` shows no change in `codegen/` or
`runtime/`. With the load average at 1.6, the perf gate alone passed:
`cargo test --release -p subscript-benchmarks --test perf_gate`, 2
passed, exit 0. The owner asked for the perf gate alone (2026-10-31).
