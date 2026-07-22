// corpus: reject/r06-structural-substitution
// purpose: Rejects structural substitution between same-shaped nominal classes.
// exercises: rejected-structural-substitution, nominal-identity
// questions: none
// expected-error: nominal types are not interchangeable

class A {
  value: i32 = 1;
}

class B {
  value: i32 = 2;
}

function read(value: A): i32 {
  return value.value;
}

export function main(): void {
  const value: B = new B();
  print(`${read(value)}`);
  unsafeDelete(value);
}
