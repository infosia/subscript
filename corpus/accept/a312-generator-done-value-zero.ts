// corpus: accept/a312-generator-done-value-zero
// purpose: A finished scalar value-class generator reads its zero fields.
// exercises: generator, iterator-result, ValueType, fixed-array, destructuring
// questions: compiler.md §145, collisions.md C8
// tsc: accepts; js-comparable: no C8: A finished value-class generator returns zero instead of undefined.
@ValueType
class Pair { v: i32 = 7; flag: boolean = true; }
function* values(): Generator<FixedArray<Pair, 2>> { yield [new Pair(), new Pair()]; }
export function main(): void {
  const iterator = values();
  iterator.next();
  const r = iterator.next();
  print(`${r.value[0].v}:${r.value[1].flag}`);
  const { value } = r;
  print(`${value[1].v}:${value[0].flag}`);
}
