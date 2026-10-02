// corpus: trap/t77-generator-done-fixed-reference
// purpose: A finished generator value must trap at the read.
// exercises: generator, iterator-result, fixed-array, reference
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// expected-trap: generator-done-value at r.value (15:15)
class Box { v: i32 = 7; }
function* values(): Generator<FixedArray<Box, 2>> { yield [new Box(), new Box()]; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  const a = r.value[0];
  const xs: Box[] = [a];
  print(`${xs.length}`);
}
