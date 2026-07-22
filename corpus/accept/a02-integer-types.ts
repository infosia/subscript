// corpus: accept/a02-integer-types
// purpose: Exercises sized numeric arithmetic and explicit conversions.
// exercises: sized-numerics, arithmetic, explicit-conversion
// questions: Q1, Q12

export function main(): void {
  const signed: i32 = -12;
  const unsigned: u32 = 20;
  const single: f32 = 1.5;
  const double: f64 = 2.25;
  const integerSum: i32 = signed + (unsigned as i32);
  const realSum: f64 = (single as f64) + double;
  const narrowed: f32 = realSum as f32;
  const converted: i32 = narrowed as i32;
  print(`${integerSum},${realSum},${converted}`);
}
