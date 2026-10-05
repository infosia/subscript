// corpus: reject/r376-function-value-default
// purpose: A call through a function value passes every parameter.
// exercises: lambda, default-parameter, function-value
// questions: compiler.md §164, collisions.md C24 row 33
// tsc: accepts
// expected-error: S100 at line 10, `h` expects 2 argument(s), got 1: a call through a function value passes every parameter

export function main(): void {
  const h = (a: i32, b: i32 = 5): i32 => a + b;
  print(`${h(1)}`);
}
