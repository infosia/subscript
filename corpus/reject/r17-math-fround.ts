// corpus: reject/r17-math-fround
// purpose: Rejects Math.fround; f32 rounding is an explicit `as f32`.
// exercises: rejected-math-subset, math-intrinsics
// questions: Q19
// expected-error: Math.fround is a JS-number op; out of the Math subset

export function main(): void {
  const x: f64 = 1.5;
  print(`${Math.fround(x)}`);
}
