// corpus: reject/r15-math-imul
// purpose: Rejects the JS-number Math ops; the language has sized integers.
// exercises: rejected-math-subset, math-intrinsics
// questions: Q19
// expected-error: Math.imul is a JS-number op; out of the Math subset

export function main(): void {
  print(`${Math.imul(1, 2)}`);
}
