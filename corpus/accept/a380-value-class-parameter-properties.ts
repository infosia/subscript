// corpus: accept/a380-value-class-parameter-properties
// purpose: The parameter-property fields of a value class come before its declared fields in the C layout.
// exercises: value-class, parameter-property, field-order, bytesOf
// questions: compiler.md §198.1 rule 2
// tsc: accepts; js-comparable: no C2: The ValueType decorator has no JavaScript shim.
@ValueType
class Pair {
  tag: i32 = 7;
  constructor(public a: i32, public b: f64) {}
}

export function main(): void {
  const q = new Pair(1, 2.5);
  let r = q;
  r.a = 5;
  print(`${q.a} ${r.a} ${r.b} ${r.tag}`);
  print(JSON.stringify(q));
  print(`${Context.bytesOf<Pair>(q).length}`);
}
