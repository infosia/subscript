// corpus: reject/r127-f32-frombits-f64-arg
// purpose: Rejects an f64 argument to Math.f32FromBits.
// exercises: math-f32-from-bits, rejected-numeric-type
// questions: §17, R28
// tsc: accepts
// expected-error: expected u32, got f64
export function main(): void {
  const value: f64 = 1.0;
  print(`${Math.f32FromBits(value)}`);
}
