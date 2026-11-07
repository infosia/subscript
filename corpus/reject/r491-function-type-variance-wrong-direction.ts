// corpus: reject/r491-function-type-variance-wrong-direction
// purpose: A function type parameter takes the nullable handle variance in one direction only, so the opposite direction stays rejected.
// exercises: function-value, nullable-reference, lambda
// questions: compiler.md §235.1 rules 3 and 6
// tsc: rejects TS2345
// expected-error: S100 at line 17, type mismatch: the argument expects

class C {
  v: i32 = 1;
}

function run(f: (g: (x: C) => C | null) => void): void {
  f((x: C): C | null => null);
}

export function main(): void {
  run((g: (x: C) => C): void => {});
}
