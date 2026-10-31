# A nullable string — `compiler.md` §218

Status: COMPLETE.

The owner selected this section and §219 on 2026-10-31. The Step 0 table
is in `specs/tracking/nullable-array.md`.

A prototype of 47 lines in 12 files at `14a143f1` closed the wrong values
of the table: 70 programs equal `node`, and 104 programs rejected or
trapped the same way on each tier. It was reverted, and it ran no gate.
It did not sweep the `Type::Str` sites.

The interpreter has two null forms, `Value::Null` and
`Value::Handle(null)`.

## Implementation

Red, on a binary built from `410faed1`: `a402` gives 27 diagnostics, the
first S011 "`string | null` is not a reference type union". `r461` gives
the same S011. `tsc` accepts each, and `node` prints `<null>` for `r461`.

Mutation of rule 2: with the checker selecting `Eq`, each tier stops at the
verifier. With the verifier rule removed too, each tier runs and gives
wrong values: `a402` line 4 is `false false true`, and the golden is
`true true false`. So the verifier rule closes the class.

`program.c` and `program.h` are byte-identical across 397 accept, trap,
warn, and example files that emit at `410faed1`. 85 need a mirror or a
module and did not emit.

The interpreter element-kind table is deleted. Enum, `Date`, and alias
elements now give kind 5 there, where they gave 0.

Hot reload: `body_edit_keeps_a_nullable_string_global` keeps a
`string | null` global across body edits in both states.

## Rule 4 sweep

225 non-test sites in `compiler/src` and `codegen/src` that read
`Type::Str` or `HandleKind::Str` at `410faed1`:

| Class | Sites |
|---|---|
| Reads the shared rule | `operator.rs` `same_scalar`, the one compare in `lower/func/expr.rs`, `cemit/arith.rs`, `interpreter/operations.rs`; `hir/collections.rs`; `check/json.rs`; `verify_instruction.rs` |
| Handle representation, no content read | `hir/shared.rs`, `hir/sites.rs`, `layout.rs`, `interpreter/memory.rs`, `cemit/emitter.rs`, `types.rs` |
| Cannot receive a nullable: rules 5, 6, 7, and 9, or a fixed `string` parameter | `.length`, a method, `for…of`, spread, `+`, a template, a key, `switch`, each boundary site |
| Gives a `string` and reads none | result and construction types, LIR signatures of fixed instructions, names |
| Test code | `#[cfg(test)]` blocks and test tables |

No site stayed unclassified.

## Phase Review fixes

`FixedArray` iteration, on a binary built from `821d2a7a`: `a403` gives 18
lines on the JIT and the ship tier, and the golden has 28. A probe with
`await` or `yield` in the body stopped with signal 11 on the JIT and gave
wrong bytes on the ship tier. The Phase Review measured the `string` form
at `ca7d8897`.

An extension of `environment_dependencies` closed the plain case only,
because a dependency does not carry a value across a suspend. The
operand form closes each case.

`program.c` across 292 accept entries that emit with no mirror: only
`a77-for-of-containers` moves among entries with an unchanged source, by
two lines of the cursor form. The LIR text snapshot does not move.

`gate full 821d2a7a dirty:26 debug 3114/0/5 release 3111/0/5 skips 2/0 clippy 5/18/11 goldens-moved 1 exit 0`. `tools/hygiene.sh` exit 0. The moved golden is `a402`, which this section adds.

## Second review fixes

Three-run values with a debug compiler, 196,608 reads of
`FixedArray<f64, 16384>` with `yield` in the body. The JIT figures hold
about 4.49 s of literal compile time.

| Shape | JIT before | JIT after | C before | C after |
|---|---|---|---|---|
| `for…of` | 5.90 s | 4.50 s | 2.1 s | 0.024 s |
| index loop | 4.49 s | 4.49 s | 0.022 s | 0.023 s |
| sync `for…of`, 1024 f64, 20.48M reads | 0.172 s | 0.180 s | 0.122 s | 0.123 s |

`program.c` across 292 accept entries: only `a77` and `a403` move. The LIR
snapshot does not move.

A rule that gave a base to any `Data` operand failed: a counted check
reads a base as an index, and a field write on a reference class gave
"replaces a counted array element" on `a359` and `a361`. So the rule
names the value kinds.

`gate full 04032626 dirty:32 debug 3120/0/5 release 3117/0/5 skips 2/0 clippy 5/18/11 goldens-moved 0 exit 0`. `tools/hygiene.sh` exit 0.
