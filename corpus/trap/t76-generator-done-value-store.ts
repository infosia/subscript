// corpus: trap/t76-generator-done-value-store
// purpose: A finished generator value must trap at the read.
// exercises: generator, iterator-result, reference
// questions: compiler.md §145, collisions.md C23
// tier-policy: both tiers trap
// tsc: accepts
// js-comparable: no C23
// node: prints 1
// expected-trap: generator-done-value at r.value (16:20)
class Box { v: i32 = 7; }
function* values(): Generator<Box> { yield new Box(); }
export function main(): void {
  const iterator: Generator<Box> = values();
  iterator.next();
  const r = iterator.next();
  const b: Box = r.value;
  const xs: Box[] = [b];
  print(`${xs.length}`);
}
