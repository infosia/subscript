// corpus: trap/t79-generator-done-destructuring
// purpose: A finished generator value must trap at the read.
// exercises: generator, iterator-result, destructuring, fixed-array, reference
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// expected-trap: generator-done-value at b (14:18)
class Box { v: i32 = 7; }
function* values(): Generator<FixedArray<Box, 2>> { yield [new Box(), new Box()]; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const { value: b } = iterator.next();
  const xs: Box[] = [b[0]];
  print(`${xs.length}`);
}
