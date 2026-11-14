// corpus: reject/r509-local-lambda-self-value
// purpose: A local lambda cannot read its pending binding as a value.
// exercises: lambda, recursion
// questions: compiler.md §284 rules 5 and 6
// tsc: accepts
// expected-error: S100 at line 8
export function main(): void {
  const local = (n: i32): void => { const other = local; other(n); }; local(0);
}
