// corpus: reject/r508-nested-lambda-calls-outer-name
// purpose: A nested lambda cannot call the pending outer binding.
// exercises: lambda, recursion
// questions: compiler.md §284 rules 5 and 6
// tsc: accepts
// expected-error: S100 at line 8
export function main(): void {
  const outer = (n: i32): void => { const inner = (): void => { outer(n); }; inner(); }; outer(0);
}
