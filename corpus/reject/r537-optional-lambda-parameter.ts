// corpus: reject/r537-optional-lambda-parameter
// purpose: An annotated lambda parameter must fit the optional function context.
// exercises: lambda, undefined, return
// questions: compiler.md §318 rule 1
// tsc: rejects TS2322
// expected-error: S100 at line 8, column 39: type mismatch: the return value expects `((i32) => f64) | undefined`, got `(string) => f64`
type CB = (x: i32) => f64;
function r(): CB | undefined { return (x: string) => 1; }
export function main(): void {}
