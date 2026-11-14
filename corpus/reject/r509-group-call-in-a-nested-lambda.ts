// corpus: reject/r509-group-call-in-a-nested-lambda
// purpose: A lambda nested in a group member cannot call the member.
// exercises: lambda, recursion
// questions: compiler.md §293 rule 7, §284 rule 5
// tsc: accepts
// expected-error: S100 at line 8
export function main(): void {
  const local = (n: i32): void => { const nested = (): void => { local(n); }; nested(); }; local(0);
}
