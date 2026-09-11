// corpus: reject/r226-field-definite-assertion
// purpose: Rejects a field whose `!` assertion stands in for a value that nothing assigns.
// exercises: class-field, definite-assignment-assertion, reference-field
// questions: compiler section 108
// tsc: accepts
// expected-error: S100 at the field
class Inner {
  value: i32 = 3;
}

class Holder {
  inner!: Inner;
}

export function main(): void {
  const holder: Holder = new Holder();
  print(`${holder.inner.value}`);
}
