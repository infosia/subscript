// corpus: reject/r224-field-without-initializer
// purpose: Rejects a scalar field with no initializer that no constructor assigns.
// exercises: class-field, field-initializer, constructor
// questions: compiler section 108
// tsc: rejects TS2564
// expected-error: S100 at the field
class Counter {
  count: i32;
}

export function main(): void {
  const counter: Counter = new Counter();
  print(`${counter.count}`);
}
