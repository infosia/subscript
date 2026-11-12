// corpus: reject/r499-a-binding-named-undefined-is-a-binding
// purpose: A binding named undefined is a binding, so a return of it from a void function returns a value.
// exercises: undefined, void, return, lambda, parameter
// questions: compiler.md §253.1 rules 1 and 4, §250
// tsc: rejects TS2322
// expected-error: S100 at line 9, a `void` function cannot return a value

function f(undefined: i32): void {
  return undefined;
}

function g(): void {
  const undefined = 3;
  return undefined;
}

const k = (undefined: i32): void => undefined;

export function main(): void {
  f(1);
  g();
  k(2);
}
