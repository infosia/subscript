// corpus: reject/r16-math-variadic-max
// purpose: Rejects Math.hypot beyond two arguments; a fold of hypot rounds differently from the N-argument form.
// exercises: rejected-math-subset, math-intrinsics
// questions: Q19, compiler.md §201
// tsc: accepts
// expected-error: a fold of `Math.hypot` rounds differently
export function main(): void {
  print(`${Math.hypot(1, 2, 3)}`);
}
