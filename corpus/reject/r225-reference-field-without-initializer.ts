// corpus: reject/r225-reference-field-without-initializer
// purpose: Rejects a reference field with no initializer that no constructor assigns.
// exercises: class-field, field-initializer, reference-field
// questions: compiler section 108
// tsc: rejects TS2564
// expected-error: S100 at the field
class Inner {
  value: i32 = 3;
}

class Holder {
  inner: Inner;
}

export function main(): void {
  const holder: Holder = new Holder();
  print(`${holder.inner.value}`);
}
