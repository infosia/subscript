// corpus: reject/r231-this-method-call-before-the-assignment
// purpose: Rejects a method call on `this` before the constructor assigns every field, because the callee reads a field that holds no value.
// exercises: class-field, constructor, this-method-call
// questions: compiler section 108
// tsc: accepts
// expected-error: S100 at the `this`
class Inner {
  value: i32 = 3;
}

class Holder {
  inner: Inner;

  constructor() {
    this.show();
    this.inner = new Inner();
  }

  show(): void {
    print(`${this.inner.value}`);
  }
}

export function main(): void {
  const holder: Holder = new Holder();
  holder.show();
}
