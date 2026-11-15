// corpus: reject/r518-undefined-type-parameter
// purpose: Rejects the reserved type parameter name undefined.
// exercises: lambda, inference, type-parameter
// questions: compiler.md §306
// tsc: rejects TS2368
// expected-error: S100 at line 8, Rejects the reserved type parameter name undefined.

function f<undefined>(x: i32): void { return undefined; }
export function main(): void {}
