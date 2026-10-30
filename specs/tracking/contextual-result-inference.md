# A generic call infers from its contextual result type — `compiler.md` §212

Status: COMPLETE.

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

## Phase Review

`compiler.md` §212.3 records the findings. A context for the right
operand of `??` in a generic body changed `a ?? 1` from `T | i32` to
`T | number`, and a form limited to call values rejected
`a ?? id(b ? 1 : 2)`, which the pin accepts. The change is reverted, and
§212.4 item 6a records the form.

`tsc` 5.9.2, measured: `nb(null)` with `nb<T>(x: Buf<T> | null)` and
`fa(null)` with `fa<T>(x: T[] | null)` give no error. `fn(null)` with
`fn<T>(x: T | null)` and `fb(null)` with `fb<T>(x: T)` give TS2322.
`fa(null)` stays S011 here: an array in a union is outside the subset,
with or without §212.

`gate full ea1514c1 dirty:9 debug 3042/0/5 release 3039/0/5 skips 2/0
clippy 5/18/11 goldens-moved 0 exit 0`. `tools/hygiene.sh` exit 0.
