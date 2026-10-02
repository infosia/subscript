// corpus: accept/a309-generator-done-null
// purpose: Tests a finished nullable-reference generator value with loose null equality.
// exercises: generator, iterator-result, nullable, equality
// questions: collisions.md C22, compiler.md §144
// tsc: accepts; js-comparable: yes
class Box { value: i32 = 7; }
function* values(): Generator<Box | null> {
  yield new Box();
}
export function main(): void {
  const iterator: Generator<Box | null> = values();
  iterator.next();
  const result = iterator.next();
  print(`${result.done}`);
  print(`${result.value == null}`);
}
