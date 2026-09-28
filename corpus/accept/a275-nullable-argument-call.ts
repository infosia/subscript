// corpus: accept/a275-nullable-argument-call
// purpose: Calls accept nullable arguments and return non-null values.
// exercises: function-call, method-call, function-value-call, nullable-argument
// questions: compiler.md §124
// tsc: accepts; js-comparable: yes
class Cell { v: i32 = 7; }
function orNew(x: Cell | null): Cell {
  if (x !== null) { return x; }
  return new Cell();
}
class Factory {
  orNew(x: Cell | null): Cell {
    if (x !== null) { return x; }
    return new Cell();
  }
}
function invoke(f: (x: Cell | null) => Cell, x: Cell | null): i32 {
  return f(x).v;
}
function exercise(c: Cell | null): void {
  print(`function=${orNew(c).v}`);
  const factory = new Factory();
  print(`method=${factory.orNew(c).v}`);
  print(`value=${invoke(orNew, c)}`);
}
export function main(): void {
  exercise(null);
  const c = new Cell();
  c.v = 9;
  exercise(c);
}
