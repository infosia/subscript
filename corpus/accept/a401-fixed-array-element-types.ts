// corpus: accept/a401-fixed-array-element-types
// purpose: Each FixedArray type has its own C type, so one length with different handle element types compiles.
// exercises: fixed-array, nullable-array, nullable-element, reference-class, class-field
// questions: compiler.md §217
// tsc: accepts; js-comparable: yes
class A {
  v: i32 = 1;
}

class B {
  v: i32 = 2;
}

class C {
  v: i32 = 3;
}

class Row {
  items: FixedArray<A, 2> = [new A(), new A()];
  labels: FixedArray<string[], 2> = [["x"], ["y", "z"]];
}

function sumA(values: FixedArray<A, 2>): i32 {
  return values[0].v + values[1].v;
}

function sumB(values: FixedArray<B, 2>): i32 {
  return values[0].v + values[1].v;
}

function makeSizes(): FixedArray<i32[], 2> {
  const sizes: FixedArray<i32[], 2> = [[1], [2, 3]];
  return sizes;
}

export function main(): void {
  const a: FixedArray<A, 2> = [new A(), new A()];
  const b: FixedArray<B, 2> = [new B(), new B()];
  print(`${sumA(a)} ${sumB(b)}`);
  const sizes = makeSizes();
  print(`${sizes[0].length} ${sizes[1].length}`);
  const names: FixedArray<string[], 2> = [["p"], ["q", "r", "s"]];
  print(`${names[1].length} ${names[0][0]}`);
  const maybe: FixedArray<i32[] | null, 2> = [[1, 2], null];
  print(`${maybe[0]?.length ?? -1} ${maybe[1] === null}`);
  const classes: FixedArray<C | null, 2> = [null, new C()];
  print(`${classes[0] === null} ${classes[1]?.v ?? -1}`);
  const nestedA: FixedArray<FixedArray<A, 2>, 2> = [[new A(), new A()], [new A(), new A()]];
  const nestedB: FixedArray<FixedArray<B, 2>, 2> = [[new B(), new B()], [new B(), new B()]];
  print(`${nestedA[1][0].v} ${nestedB[1][1].v}`);
  const plain: FixedArray<() => i32, 2> = [(): i32 => 1, (): i32 => 2];
  const unary: FixedArray<(x: i32) => i32, 2> = [(x: i32): i32 => x, (x: i32): i32 => x + 1];
  print(`${plain[1]()} ${unary[1](2)}`);
  const row = new Row();
  print(`${sumA(row.items)} ${row.labels[1].length}`);
}
