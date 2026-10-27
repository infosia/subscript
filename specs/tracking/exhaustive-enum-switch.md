# A switch over every enum member ends the flow — `compiler.md` §200

Status: landed.

## Measurement at the pin `409a05f5`

Tools: `tsc` 5.9.2 with this project's options, `node` v24.22.1
(TypeScript `transpileModule`, ES2022), and `target/debug/subscript`
built from the pin.

| Entry | `tsc` | subscript at the pin |
|---|---|---|
| `a383-exhaustive-enum-switch` | no error | S100 "not all paths return a value" ×3 |
| `t112-enum-switch-non-member-value` | no error | S100 "not all paths return a value" |

With one member missing, `tsc` gives TS2366. `node` prints the `a383`
golden: `0 4 3`, `low high`, `9 2`.

A value that is not a member reaches a switch: `Context.fromBytes<V>`
of the bytes `[9, 0, 0, 0]`, for a value class `V` with one field of
a two-member enum, gives the field value `9` (`subscript run`).

## Landing

`gate full dc33ed04 dirty:20 debug 2947/0/5 release 2943/1/5 skips
2/0 clippy 5/18/11 goldens-moved 0 exit 1`. The one failure is the
release perf gate: `a22 ship-tier 1.53x of C, limit 1.50x`. Its
eleven ship-tier runs spread from 6.18 ms to 17.63 ms (140%), and the
noise check flagged six subjects. The load average was about 8 then
113, from `mediaanalysisd`, `ANECompilerService`, and `mdsync` after
an OS update. §200 changes HIR only, and neither `a22` nor `collect`
has an enum or a switch. With the load average at 1.97, the perf gate
alone passed: `cargo test --release -p subscript-benchmarks --test
perf_gate`, 2 passed, exit 0.
