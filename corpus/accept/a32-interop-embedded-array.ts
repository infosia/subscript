// corpus: accept/a32-interop-embedded-array
// purpose: Passes a descriptor-embedded (count, pointer) array field zero-copy inside a boundary struct.
// exercises: interop-embedded-array, descriptor-embedded-pair, boundary-struct-by-value, foreign-call
// questions: Q13, Q4

// A production header spells an array as adjacent `size_t drawsCount;
// const uint32_t* draws;` fields inside a larger struct (SubDrawList,
// compiler.md §13.2). The mirror elides the count and exposes
// `draws: u32[]`; the lowering reconstructs the C pair (count, ptr)
// count-first from the one array, filling both from its own backing store
// (zero-copy). `layer` makes the struct larger than 16 bytes, so it takes
// the by-reference boundary-struct path. subDrawListTotal sums
// `layer + every draw`: 5 + (10 + 20 + 30 + 40) = 105.

export function main(): void {
  const draws: u32[] = [10, 20, 30, 40];
  const list: SubDrawList = new SubDrawList(5, draws);
  const total: i32 = subDrawListTotal(list);
  print(`${total}`);
}
