// corpus: reject/r498-a-lambda-in-a-type-parameter-place-names-no-typescript-form
// purpose: A lambda in a place whose declared type is a bare type parameter has no context, so tsc and subscript reject its parameter, and the diagnostic carries no divergence block.
// exercises: lambda, generic-function, generic-class, nullable
// questions: compiler.md §252.1 rules 1 and 2
// tsc: rejects TS7006
// expected-error: S100 at line 26, parameters require a type annotation

function wrap<T>(f: T): T {
  return f;
}

class Box<T> {
  f: T;
  constructor(f: T) {
    this.f = f;
  }
}

function nn<T>(f: T | null): void {
  if (f !== null) {
    print("nn");
  }
}

export function main(): void {
  wrap((x) => print(`w ${x}`));
  const b = new Box((x) => print(`b ${x}`));
  nn((x) => print(`n ${x}`));
}
