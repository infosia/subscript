// corpus: reject/r451-class-implements
// purpose: An implements clause gives no conversion between nominal classes, so the checker rejects it.
// exercises: class-declarations, nominal-types
// questions: compiler.md §205
// tsc: accepts
// expected-error: S100, a class cannot declare `implements`
class Shape {
  area(): f64 {
    return 0;
  }
}

class Square implements Shape {
  side: f64 = 2;
  area(): f64 {
    return this.side * this.side;
  }
}

export function main(): void {
  print(`${new Square().area()}`);
}
