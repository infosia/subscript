// corpus: reject/r455-interface-value-type
// purpose: An interface is a constraint only; a parameter of an interface type would need a dispatch by the run-time class.
// exercises: interface
// questions: compiler.md §208.1 rule 5
// tsc: accepts
// expected-error: S100, interface `Shape` is a constraint only
interface Shape {
  area(): f64;
}

class Square implements Shape {
  area(): f64 {
    return 4;
  }
}

function areaOf(s: Shape): f64 {
  return s.area();
}

export function main(): void {
  print(`${areaOf(new Square())}`);
}
