// corpus: reject/r179-generic-method-without-type-args
// purpose: Rejects a generic method call that supplies no type arguments.
// exercises: generic-method, type-arguments
// questions: §82.4, §64
// tsc: accepts
// expected-error: S100 at the method name
class Box {
  identity<T>(value: T): T {
    return value;
  }
}

export function main(): void {
  const box: Box = new Box();
  print(`${box.identity(1)}`);
}
