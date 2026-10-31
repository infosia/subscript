// corpus: trap/t113-fixed-array-traversal-element-removed
// purpose: A traversal of an element of a dynamic array keeps its index, so a step after a removal stops with the trap of an index read.
// exercises: for-of, fixed-array, pop, index-read
// questions: compiler.md §220.1 rule 6
// tsc: accepts
// js-comparable: no C2: A dynamic-array element subject keeps its index, and node keeps the array object.
// tier-policy: all three tiers trap
// expected-trap: index-out-of-bounds at 12:19
export function main(): void {
  const m: FixedArray<i32, 2>[] = [[1, 2]];
  let n: i32 = 0;
  for (const x of m[0]) {
    print(`step ${x}`);
    m.pop();
    n += 1;
  }
  print(`end ${n}`);
}
