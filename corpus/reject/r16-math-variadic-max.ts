// corpus: reject/r16-math-variadic-max
// purpose: Rejects the lib's variadic Math.max beyond two arguments.
// exercises: rejected-math-subset, math-intrinsics
// questions: Q19
// expected-error: Math.max takes exactly two arguments

export function main(): void {
  print(`${Math.max(1, 2, 3)}`);
}
