// corpus: reject/r283-generic-body-unknown-name
// purpose: Rejects an unknown name in a generic function body that no call instantiates.
// exercises: generic-function, unknown-name
// questions: compiler.md §135
// tsc: rejects TS2304
// expected-error: S016 at line 8, the unknown function nope
function pick<T>(value: T): i32 {
  return nope();
}

export function main(): void {
  print("ok");
}
