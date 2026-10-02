// corpus: accept/a310-generator-done-reference-controls
// purpose: Nullable values and live reference iteration do not trap.
// exercises: generator, iterator-result, nullable, destructuring, for-of
// questions: compiler.md §145, collisions.md C22
// tsc: accepts; js-comparable: yes
class Box { v: i32 = 7; }
function* nullable(): Generator<Box | null> { yield new Box(); }
function* boxes(): Generator<Box> { yield new Box(); yield new Box(); }
export function main(): void {
  const iterator = nullable();
  iterator.next();
  const r = iterator.next();
  print(`${r.value == null}`);
  const { value } = r;
  print(`${value == null}`);
  const live = boxes();
  let item = live.next();
  while (!item.done) {
    const { value: box } = item;
    print(`${box.v}`);
    item = live.next();
  }
  for (const box of boxes()) { print(`${box.v}`); }
}
