// corpus: reject/r510-mutually-recursive-local-lambdas
// purpose: Two local lambdas cannot call each other.
// exercises: lambda, recursion
// questions: compiler.md §284 rules 5 and 6
// tsc: accepts
// expected-error: S100 at line 8
export function main(): void {
  const first = (n: i32): void => { if (n > 0) second(n - 1); }; const second = (n: i32): void => { first(n); }; first(1);
}
