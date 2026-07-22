// corpus: reject/r08-bare-number
// purpose: Rejects the unsized default numeric type.
// exercises: rejected-bare-number, sized-numerics
// questions: none
// expected-error: no default numeric type; use a sized type

const count: number = 3;

export function main(): void {
  print(`${count}`);
}
