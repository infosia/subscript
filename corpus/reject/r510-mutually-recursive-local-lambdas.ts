// corpus: reject/r510-mutually-recursive-local-lambdas
// purpose: A statement separates two local lambdas into different groups.
// exercises: lambda, recursion
// questions: compiler.md §293 rule 7
// tsc: accepts
// expected-error: S100 at line 8
export function main(): void {
  const first = (n: i32): void => { if (n > 0) second(n - 1); };
  print("between");
  const second = (n: i32): void => { first(n); };
  first(1);
}
