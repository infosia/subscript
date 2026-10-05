// corpus: reject/r375-default-before-required
// purpose: A default before a required parameter does not reduce the required count.
// exercises: default-parameter, argument-count
// questions: compiler.md §164
// tsc: rejects TS2554
// expected-error: S100 at line 10, `f` expects 2 argument(s) (2 required), got 1

function f(a: i32 = 1, b: i32): i32 { return a + b; }
export function main(): void {
  print(`${f(2)}`);
}
