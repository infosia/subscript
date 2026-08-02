// corpus: reject/r116-object-literal-nullable-class
// purpose: Keeps object literals nominally rejected through nullable plain-class contexts.
// exercises: object-literal, nullable-class, nominal-class
// questions: Q33, R17, C1, C7
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript accepts {} for an empty class, including through `| null`.
// expected-error: S005 at the object literal

class PlainClass {}

export function main(): void {
  const value: PlainClass | null = {};
  print(`${value !== null}`);
}
