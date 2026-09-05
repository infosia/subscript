// corpus: accept/a33-interop-flags
// interpreter: no — calls the synthetic native interop library
// purpose: Combines flag-typedef members with | (Q18) and passes the combined u64 to a foreign bit test.
// exercises: interop-flags, flag-typedef, u64-bitwise, foreign-call
// questions: Q13, Q18
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// A flag typedef `SubAccess` (= u64) with `static const` members, mapped by
// bindgen to a `u64` alias plus folded `declare const` values whose bits
// come from the C header (compiler.md §13.2). The members combine with `|`
// as true 64-bit (Q18); the combined mask crosses the boundary to
// subAccessMatches, which reports whether every required bit is set.
// READ | WRITE contains READ (1) but not EXEC (0).

export function main(): void {
  const mask: u64 = SUB_ACCESS_READ | SUB_ACCESS_WRITE;
  print(`${subAccessMatches(mask, SUB_ACCESS_READ)}`);
  print(`${subAccessMatches(mask, SUB_ACCESS_EXEC)}`);
}
