// corpus: reject/r222-pattern-computed-field-name
// purpose: Rejects a computed field name in a binding pattern.
// exercises: binding-pattern, field-name
// questions: Q30, compiler section 107
// tsc: accepts
// expected-error: S100 naming the identifier a field pattern requires
class Point {
  x: i32 = 1;
}
export function main(): void {
  const point: Point = new Point();
  const key = "x";
  const { [key]: value } = point;
  print(`${value}`);
}
