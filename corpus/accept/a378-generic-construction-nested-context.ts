// corpus: accept/a378-generic-construction-nested-context
// purpose: A contextual type reaches a nested generic construction through the argument of the outer one.
// exercises: generics, type-inference, constructor, Map
// questions: compiler.md §197.1 rule 3, compiler.md §197.3
// tsc: accepts; js-comparable: yes
class Box<T> {
  v: T;
  constructor(v: T) {
    this.v = v;
  }
}

function take(b: Box<Box<f64>>): f64 {
  return b.v.v;
}

export function main(): void {
  const a: Box<Box<f64>> = new Box(new Box(1));
  const big: Box<Box<i64>> = new Box(new Box(3000000000));
  const counts: Box<Map<string, i32>> = new Box(new Map());
  counts.v.set("k", 2);
  print(`${a.v.v} ${big.v.v} ${counts.v.size} ${take(new Box(new Box(4)))}`);
}
