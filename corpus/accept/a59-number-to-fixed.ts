// corpus: accept/a59-number-to-fixed
// purpose: Pins Q25 toFixed rounding, signs, special values, the 1e21
//          Q14 fallback, and f32 receiver widening on both tiers.
// exercises: number-to-fixed, q14-exponent-threshold
// questions: Q14, Q25

export function main(): void {
  print(`stored-half ${(1.005).toFixed(2)}`);
  print(`ties ${(2.5).toFixed(0)} ${(-2.5).toFixed(0)}`);
  print(`signs ${(-0.0).toFixed(2)} ${(-0.0001).toFixed(2)} ${(-12.34).toFixed(3)}`);
  print(`padding ${(12.34).toFixed(4)} ${(0.5).toFixed(3)}`);
  print(`fallback ${(1e21).toFixed(2)}`);
  print(`specials ${Number.NaN.toFixed(2)} ${Number.POSITIVE_INFINITY.toFixed(2)} ${Number.NEGATIVE_INFINITY.toFixed(2)}`);
  const half: f32 = 1.25;
  print(`f32 ${half.toFixed(1)}`);
}
