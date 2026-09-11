// corpus: reject/r230-field-read-before-its-assignment
// purpose: Rejects a read of a field before the constructor statement that assigns it.
// exercises: class-field, constructor, this-read
// questions: compiler section 108
// tsc: rejects TS2565
// expected-error: S100 at the `this`
class Inner {
  value: i32 = 3;
}

class Holder {
  inner: Inner;

  constructor() {
    print(`${this.inner.value}`);
    this.inner = new Inner();
  }
}

export function main(): void {
  const holder: Holder = new Holder();
  print(`${holder.inner.value}`);
}
