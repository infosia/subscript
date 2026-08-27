// corpus: accept/a36-interop-chained-flags
// purpose: Combines members of a TWO-LEVEL flag alias with | (Q18) and passes the combined u64 to a foreign bit test.
// exercises: interop-flags, chained-flag-typedef, u64-bitwise, foreign-call
// questions: Q13, Q18
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// §14.1 chained flag alias. The C header spells the flag typedef as two
// typedef levels: `typedef uint64_t SubStageBits; typedef SubStageBits
// SubStageFlags;`. bindgen follows the chain to the underlying integer,
// mapping SubStageFlags to a `u64` alias plus folded `declare const`
// members whose bits come from the C header — exactly the one-level
// SubAccess flag of a33, but resolved through two levels. The members
// combine with `|` as true 64-bit (Q18); the combined mask crosses the
// boundary to subStageMatches, which reports whether every required bit is
// set. VERTEX | FRAGMENT contains VERTEX (1) but not COMPUTE (4).

export function main(): void {
  const mask: u64 = SUB_STAGE_VERTEX | SUB_STAGE_FRAGMENT;
  print(`${subStageMatches(mask, SUB_STAGE_VERTEX)}`);
  print(`${subStageMatches(mask, SUB_STAGE_COMPUTE)}`);
}
