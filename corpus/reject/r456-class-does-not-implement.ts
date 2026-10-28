// corpus: reject/r456-class-does-not-implement
// purpose: A class that declares an interface must have each member of it with the same signature.
// exercises: interface, implements
// questions: compiler.md §208.1 rule 3
// tsc: rejects TS2420
// expected-error: S100, class `Square` does not implement `area` of interface `Shape`
interface Shape {
  area(): f64;
}

class Square implements Shape {
  side(): f64 {
    return 2;
  }
}

export function main(): void {
  print(`${new Square().side()}`);
}
