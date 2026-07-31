// corpus: reject/r92-literal-for-unmarked-class
// purpose: Keeps object literals from structurally satisfying unmarked nominal classes.
// exercises: object-literal, nominal-class, descriptor-marker
// questions: Q33, C1
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts this structurally.
// expected-error: S005 at the object literal

class UnmarkedDescriptorShape {
  value!: i32;
}

export function main(): void {
  const unmarked: UnmarkedDescriptorShape = { value: 1 };
  print(`${unmarked.value}`);
}
