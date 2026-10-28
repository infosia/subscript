// corpus: reject/r457-constraint-needs-implements
// purpose: A type argument for T extends I must be a class that declares implements I; a class with the same members is not enough (C1).
// exercises: interface, generic-constraint, nominal-types
// questions: compiler.md §208.1 rule 4
// tsc: accepts
// expected-error: S100, `Square` does not declare `implements Shape`
interface Shape {
  area(): f64;
}

class Square {
  area(): f64 {
    return 4;
  }
}

function areaOf<T extends Shape>(s: T): f64 {
  return s.area();
}

export function main(): void {
  print(`${areaOf(new Square())}`);
}
