# §90 — no public entry point panics or faults on any input

Status: **in progress.** Contract: `specs/blocks/compiler.md` §90
(`3782747`). Origin: the owner's question of 2026-09-06.

## The measurement (at `68b4213`, this host)

Static, clippy on the lib and bin targets with `unwrap_used`,
`expect_used`, `panic`, `unreachable`, `todo`, `unimplemented`,
`indexing_slicing`, `arithmetic_side_effects`:

| crate | `panic!` | `unreachable!` | `unwrap` | `expect` | index/slice | arithmetic |
|---|---:|---:|---:|---:|---:|---:|
| compiler | 0 | 4 | 35 | 48 | 266 | 90 |
| runtime | 0 | 0 | 0 | 4 | 96 | 265 |
| codegen | 0 | 17 | 1 | 69 | 565 | 234 |
| bindgen | 0 | 0 | 0 | 4 | 22 | 25 |
| cli | 0 | 0 | 0 | 1 | 21 | 8 |

Every `unreachable!` and `expect` message names an internal
invariant ("validated above", "rejected above", "write to String",
"a synthetic prefix must have an owner"); 35 of the compiler's
`unwrap` are `write!` into a `String` in `lir_text.rs`.

Dynamic, release, one child process per input (a temporary example
binary, deleted): 590 `.ts` sources under `corpus/` and `examples/`,
each with 12 truncations at character boundaries, 12 single-byte
replacements, and up to 12 single-line deletions, through
`check_program` → `check_warnings` → `render_*` → `lower_module` →
`lir_text::print_module` → `emit_c`, and `interpreter::interpret`
for the unmodified entries; every `.h` under `corpus/interop/` and
`examples/` with 12 truncations and a garbage header through
`bindgen::generate`; 12 malformed argument lists through
`cli::execute`. **15,898 runs, 15,897 completed, 0 panics caught, 1
fault** (SIGSEGV): `a09-enums.ts` truncated to
`enum Status {\n  Ready`.

The fault: `swc_ecma_parser` 6.0.2 `parse_ts_enum_member`
(`src/parser/typescript.rs:788`) calls `bump!` in its error-recovery
branch with no current token. `bump!` is `debug_assert!(knows_cur)`
then `input.bump()`, whose `None` arm is `debug_unreachable!`: a
panic in debug ("parser should not call bump() without knowing
current token"), `unreachable_unchecked` in release. Measured:
`target/debug/subscript check` exit 101 with that message;
`target/release/subscript check` exit 139. `catch_unwind` around the
parser: debug catches; release faults the same way (the first probe
version, in-process, died at that input).

Excluded: the 213 runtime `extern "C"` functions (invariant 6;
§90.1 rule 5).
