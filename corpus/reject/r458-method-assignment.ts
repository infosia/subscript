// corpus: reject/r458-method-assignment
// purpose: A method is a C function with no slot, so an assignment to it is rejected with a text that names the method.
// exercises: class-members, assignment
// questions: compiler.md §209
// tsc: accepts
// expected-error: S100, method `v` of `C` cannot be assigned
class C {
  v(): i32 {
    return 1;
  }
}

function two(): i32 {
  return 2;
}

export function main(): void {
  const c = new C();
  c.v = two;
  print(`${c.v()}`);
}
