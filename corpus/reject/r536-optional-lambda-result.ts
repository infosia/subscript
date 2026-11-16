// corpus: reject/r536-optional-lambda-result
// purpose: The optional function context checks the lambda result.
// exercises: lambda, undefined, return
// questions: compiler.md §318 rule 1
// tsc: rejects TS2322
// expected-error: S100 at line 8, column 46: type mismatch: the lambda body expects `i32`, got `string`
type CB = (x: i32) => i32;
function r(): CB | undefined { return (x) => "wrong"; }
export function main(): void {}
