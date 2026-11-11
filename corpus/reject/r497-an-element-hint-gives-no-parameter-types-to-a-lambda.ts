// corpus: reject/r497-an-element-hint-gives-no-parameter-types-to-a-lambda
// purpose: The type of an earlier element of a context-free array literal gives no parameter types to a lambda, at any depth.
// exercises: lambda, array-literal, function-value, generic-function
// questions: compiler.md §249.1 rules 1 and 1a, §212 rule 1a
// tsc: rejects TS7006
// expected-error: S100 at line 13, parameters require a type annotation

function one(x: i32): void {
  print(`one ${x}`);
}

export function main(): void {
  const a1 = [one, (x) => print(`a1 ${x}`)];
  const a2 = [(x: i32): void => {}, (x) => print(`a2 ${x}`)];
  const ns = [[one], [(x) => print(`ns ${x}`)]];
  a1[1](1);
  a2[1](2);
  ns[1][0](3);
}

type CB = (x: i32) => void;

function mk(): CB {
  return one;
}

function wrap<T>(f: T): T {
  return f;
}

class Box<T> {
  f: T;
  constructor(f: T) {
    this.f = f;
  }
}

function mkb(): Box<CB> {
  return new Box<CB>(one);
}

export function more(): void {
  const w1 = [mk, () => wrap((x) => print(`w1 ${x}`))];
  const w2 = [mk, () => { return wrap((x) => print(`w2 ${x}`)); }];
  const b1 = new Box<CB>(one);
  const n1 = [b1, new Box((x) => print(`n1 ${x}`))];
  const n2 = [mkb, () => new Box((x) => print(`n2 ${x}`))];
  const o1 = [[one], Array.of((x) => print(`o1 ${x}`))];
}
