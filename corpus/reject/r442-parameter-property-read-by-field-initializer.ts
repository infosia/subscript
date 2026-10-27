// corpus: reject/r442-parameter-property-read-by-field-initializer
// purpose: A field initializer runs before a parameter property is assigned, so it cannot read it.
// exercises: class, parameter-property, field-initializer
// questions: compiler.md §198.1 rule 4
// tsc: rejects TS2729
// expected-error: S100, `this.x` must read an earlier instance field with an initializer
class Cell {
  doubled: i32 = this.x * 2;
  constructor(public x: i32) {}
}

export function main(): void {
  print(`${new Cell(3).doubled}`);
}
