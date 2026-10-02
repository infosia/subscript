// corpus: accept/a311-generator-done-scalar-zero
// purpose: A finished integer generator keeps its zero value.
// exercises: generator, iterator-result, scalar, member-read
// questions: compiler.md §145, collisions.md C8
// tsc: accepts; js-comparable: no C8: A finished integer generator returns zero instead of undefined.
// node: prints undefined twice
function* values(): Generator<i32> { yield 7; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  print(`${r.value}`);
  const value = r.value;
  print(`${value}`);
}
