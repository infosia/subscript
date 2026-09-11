// corpus: reject/r232-this-argument-before-the-assignment
// purpose: Rejects `this` as a call argument before the constructor assigns every field, because the callee reads a field that holds no value.
// exercises: class-field, constructor, this-argument
// questions: compiler section 108
// tsc: accepts
// expected-error: S100 at the `this`
class Inner {
  value: i32 = 3;
}

class Holder {
  inner: Inner;

  constructor() {
    show(this);
    this.inner = new Inner();
  }
}

function show(holder: Holder): void {
  print(`${holder.inner.value}`);
}

export function main(): void {
  const holder: Holder = new Holder();
  show(holder);
}
