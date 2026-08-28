// corpus: accept/a167-saturating-float-casts
// purpose: Saturates float-to-integer casts and converts NaN to zero.
// exercises: float-to-integer-cast, saturation, nan, signed-integer, unsigned-integer
// questions: §68, C3
// tsc: accepts; js-comparable: no C3: A TypeScript assertion does not convert its number value.

export function main(): void {
  const zero: f64 = 0.0;
  const nan: f64 = zero / zero;
  print(`${1e10 as i32}`);
  print(`${(-1.0) as u32}`);
  print(`${300.0 as i8}`);
  print(`${nan as i32}`);
}
