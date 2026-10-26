// corpus: reject/r427-undefined-scalar-result
// purpose: A scalar T | undefined result is rejected; a scalar has no absence value.
// exercises: undefined, scalar
// questions: compiler.md §195
// tsc: accepts
// expected-error: S011, `i32 | undefined` is not a reference type union
class Box {
  v: i32 = 1;
}
function pick(f: boolean): i32 | undefined {
  if (f) {
    return 1;
  }
  return undefined;
}

export function main(): void {
  print(`${pick(false) === undefined}`);
}
