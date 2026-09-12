<!-- §78 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 78. The MINOR consolidation pass

*(Owner decision, 2026-08-30.)*

**Rule 1.** A MINOR consolidation changes no observable behaviour. No
corpus golden, `.expected`, LIR golden text, warning output, diagnostic
text, or runtime symbol set moves. A rule below names each permitted
move.

**Rule 2.** A consolidation that removes a runtime symbol (`Rm19`: one
`subscript_rt_assoc_*` symbol per Map/Set operation; `Rm32`:
`subscript_rt_str_method_concat` deleted) changes both code generators
and the runtime in one round. The round re-derives the emitted C for
every corpus entry, and the differential gate is the check.

**Rule 3.** The `Template` instruction carries a `FormatKind` per piece
(`Gm18`, core principle 8); the transcribers read it and keep no type
table. The LIR golden text gains the field; that move is the only
golden move the pass allows, and the round reports the diff.

**Rule 4.** `compiler/src/lir.rs` holds `IntrinsicOperation::runtime_symbol()`
(`Gm17`) next to the contract enum. `codegen/src/lir_types.rs` holds
the one LIR-to-runtime trap-kind map (`Gm7`), because `compiler/` does
not depend on `runtime/`. Every transcriber and the interpreter read
both.

**Rule 5.** If the round finds an item unsound, or the item needs a
decision this section does not give, the round reports the item and
leaves it. The round does not stop for it.

**Recorded candidate (not decided).** W003 on a userdata argument that
is a conditional (`c ? new X() : keep`): the pass measured that
`flow_leaves()` widened the warning to that shape and restored the old
`Local`/`Cast` walk. A warn-corpus entry decides it.
