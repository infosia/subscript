// corpus: accept/a196-zero-format
// purpose: Checks compiler section 95 zero-format semantics.
// exercises: string-methods, q14-formatting
// questions: Q14
// tsc: accepts; js-comparable: yes
export function main(): void {
  const single: f32 = -0.0;
  const double: f64 = -0.0;
  print(`${single}|${double}|${0.0}`);
  print(`${1.0 / single}|${1.0 / double}`);
  print(double.toFixed(2));
}
