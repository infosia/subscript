// corpus: reject/r218-pattern-object-rest
// purpose: Rejects a rest element in a field binding pattern.
// exercises: binding-pattern, rest-element, missing-object-type
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the missing result shape
class Point {
  x: i32 = 1;
  y: i32 = 2;
}
export function main(): void {
  const point: Point = new Point();
  const { x, ...rest } = point;
  print(`${x}`);
}
