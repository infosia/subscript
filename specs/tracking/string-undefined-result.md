# A string result can be `undefined`, and equality narrows — `compiler.md` §219

Status: COMPLETE.

The Step 0 of `specs/tracking/nullable-array.md` added `Str` to
`map_get_value_ok`: one line gave `string | undefined` results, equal to
`node` on the three tiers. It failed one pin,
`tests::removal::pop_and_shift_give_undefined_only_for_the_map_get_set`.

`compiler/src/check/narrowing.rs` reads a null operand only, so
`q === "lit"` gives no narrowing, and `C` equality gives none.

## Implementation

Red at `1ce1efc1`, whose checker equals the pin `bbd38549`: `a404` gives 38
errors, 9 × S014 on `get`, `find`, and `findLast`, 4 × S100 on `??`, 6 × S100
on `===` of `C | null` and `C`, 16 × S011, and 3 × S012. `r463` is
accepted.

No accept, trap, warn, or example entry moves. `corpus/trap` has no entry
that reads an empty `string[]` `pop` or `shift`.

A matrix of 57 forms against `tsc` differs only by a stated rule: C17, C24
row 32, §218.1 rule 5, and S009.

`gate full 152f26b2 dirty:8 debug 3131/0/5 release 3128/0/5 skips 2/0 clippy 5/18/11 goldens-moved 0 exit 0`. `tools/hygiene.sh` exit 0.
