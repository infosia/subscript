// corpus: reject/r201-new-class-spread-variadic
// purpose: Rejects a spread argument in a class construction.
// exercises: construction, spread-argument, variadic
// questions: Q30, compiler section 103
// tsc: accepts
// expected-error: S014 naming the missing variadic parameters
class Point {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}
export function main(): void {
  const parts: i32[] = [1, 2];
  const point: Point = new Point(...[parts[0], parts[1]]);
  print(`${point.x}`);
}
