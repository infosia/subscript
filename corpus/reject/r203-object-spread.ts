// corpus: reject/r203-object-spread
// purpose: Rejects an object literal that spreads a class instance.
// exercises: object-literal, spread
// questions: Q30, compiler section 103
// tsc: accepts
// expected-error: S100 naming the undecided object-literal surface
class Point {
  x: i32;
  constructor(x: i32) {
    this.x = x;
  }
}
export function main(): void {
  const point: Point = new Point(1);
  const copy = { ...point };
  print(`${copy.x}`);
}
