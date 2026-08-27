// corpus: accept/a21-methods
// purpose: Calls methods on a value struct and a reference class.
// exercises: value-method, reference-method, receiver
// questions: Q1, Q2, Q6, Q12
// tsc: accepts; js-comparable: no C2 Q6: The CStruct decorator has no JavaScript shim.
@CStruct
class Point {
  x: f32;
  y: f32;

  constructor(x: f32, y: f32) {
    this.x = x;
    this.y = y;
  }

  sum(): f32 {
    return this.x + this.y;
  }
}

class Accumulator {
  total: f32;

  constructor() {
    this.total = 0.0;
  }

  add(value: f32): void {
    this.total += value;
  }
}

export function main(): void {
  const point: Point = new Point(2.5, 3.5);
  const accumulator: Accumulator = new Accumulator();
  accumulator.add(point.sum());
  print(`${accumulator.total}`);
  Context.free(accumulator);
}
