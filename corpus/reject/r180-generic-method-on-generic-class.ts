// corpus: reject/r180-generic-method-on-generic-class
// purpose: Rejects a generic method declared on a generic class.
// exercises: generic-method, generic-class
// questions: §82.4, §64
// tsc: accepts
// expected-error: S100 at the method declaration
class Holder<T> {
  value: T;

  constructor(value: T) {
    this.value = value;
  }

  pick<U>(other: U): U {
    return other;
  }
}

export function main(): void {
  const holder: Holder<i32> = new Holder<i32>(1);
  print(`${holder.value}`);
}
