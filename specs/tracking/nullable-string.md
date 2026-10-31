# A nullable string — `compiler.md` §218

Status: implemented; Phase Review open.

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
