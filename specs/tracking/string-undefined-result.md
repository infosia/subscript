# A string result can be `undefined`, and equality narrows — `compiler.md` §219

Status: contract.

The Step 0 of `specs/tracking/nullable-array.md` added `Str` to
`map_get_value_ok`: one line gave `string | undefined` results, equal to
`node` on the three tiers. It failed one pin,
`tests::removal::pop_and_shift_give_undefined_only_for_the_map_get_set`.

`compiler/src/check/narrowing.rs` reads a null operand only, so
`q === "lit"` gives no narrowing, and `C` equality gives none.
